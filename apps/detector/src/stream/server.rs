//! HLS stream server
//!
//! Serves on-demand HLS streams. Pipeline starts when first client connects,
//! stops when no clients for timeout period.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::RwLock;
use tokio::fs;
use tracing::{debug, error, info, warn};

use crate::config::CameraConfig;
use crate::AppState;

/// Active stream state
struct StreamState {
    pipeline: gst::Pipeline,
    last_access: Instant,
    output_dir: PathBuf,
}

/// Manages on-demand HLS streams
pub struct StreamServer {
    state: Arc<AppState>,
    /// Active streams by camera_id
    streams: Arc<RwLock<HashMap<String, StreamState>>>,
    /// Base directory for stream output
    base_dir: PathBuf,
    /// Idle timeout in seconds before stopping stream
    idle_timeout_secs: u64,
}

impl StreamServer {
    pub fn new(state: Arc<AppState>) -> Self {
        let base_dir = std::env::temp_dir().join("fire-detect-streams");
        std::fs::create_dir_all(&base_dir).ok();
        
        Self {
            state,
            streams: Arc::new(RwLock::new(HashMap::new())),
            base_dir,
            idle_timeout_secs: 60,
        }
    }

    /// Get or create stream for camera
    fn get_or_create_stream(&self, camera_id: &str) -> Result<PathBuf, String> {
        let config = self
            .state
            .config
            .cameras
            .iter()
            .find(|c| c.camera_id == camera_id && c.enabled)
            .ok_or_else(|| format!("Camera {} not found or disabled", camera_id))?;

        let output_dir = self.base_dir.join(camera_id);
        
        {
            let mut streams = self.streams.write();
            if let Some(stream) = streams.get_mut(camera_id) {
                stream.last_access = Instant::now();
                return Ok(stream.output_dir.clone());
            }
        }

        // Create new stream
        std::fs::create_dir_all(&output_dir)
            .map_err(|e| format!("Failed to create stream dir: {}", e))?;

        let pipeline = Self::create_hls_pipeline(config, &output_dir)?;
        pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| format!("Failed to start pipeline: {}", e))?;

        let stream = StreamState {
            pipeline,
            last_access: Instant::now(),
            output_dir: output_dir.clone(),
        };

        self.streams.write().insert(camera_id.to_string(), stream);
        info!(camera_id = %camera_id, "Started HLS stream");

        Ok(output_dir)
    }

    fn create_hls_pipeline(config: &CameraConfig, output_dir: &PathBuf) -> Result<gst::Pipeline, String> {
        let playlist_path = output_dir.join("playlist.m3u8");
        let segment_pattern = output_dir.join("segment%05d.ts");

        let pipeline_str = format!(
            r#"
            rtspsrc location="{}" latency=100 protocols=tcp name=src
            ! rtph264depay
            ! h264parse
            ! hlssink2 target-duration=2 max-files=5
                playlist-location="{}"
                location="{}"
            "#,
            config.rtsp_url.replace('"', "\\\""),
            playlist_path.display(),
            segment_pattern.display()
        )
        .trim()
        .replace('\n', " ");

        let pipeline = gst::parse::launch(&pipeline_str)
            .map_err(|e| format!("Failed to create pipeline: {}", e))?
            .dynamic_cast::<gst::Pipeline>()
            .map_err(|_| "Failed to cast to Pipeline".to_string())?;

        Ok(pipeline)
    }

    /// Cleanup idle streams (call periodically)
    pub fn cleanup_idle(&self) {
        let mut streams = self.streams.write();
        let mut to_remove = Vec::new();
        
        for (camera_id, stream) in streams.iter() {
            if stream.last_access.elapsed().as_secs() > self.idle_timeout_secs {
                to_remove.push(camera_id.clone());
            }
        }

        for camera_id in to_remove {
            if let Some(stream) = streams.remove(&camera_id) {
                let _ = stream.pipeline.set_state(gst::State::Null);
                info!(camera_id = %camera_id, "Stopped idle HLS stream");
            }
        }
    }

    /// Build router for stream endpoints
    pub fn router(&self) -> Router {
        let state1 = self.state.clone();
        let streams1 = self.streams.clone();
        let base_dir1 = self.base_dir.clone();
        
        let state2 = self.state.clone();
        let streams2 = self.streams.clone();
        let base_dir2 = self.base_dir.clone();

        Router::new()
            .route(
                "/stream/:camera_id/playlist.m3u8",
                get(move |Path(camera_id): Path<String>| {
                    let state = state1.clone();
                    let streams = streams1.clone();
                    let base_dir = base_dir1.clone();
                    async move {
                        serve_playlist(&state, &streams, &base_dir, &camera_id).await
                    }
                }),
            )
            .route(
                "/stream/:camera_id/:segment",
                get(move |Path((camera_id, segment)): Path<(String, String)>| {
                    let state = state2.clone();
                    let streams = streams2.clone();
                    let base_dir = base_dir2.clone();
                    async move {
                        serve_segment(&state, &streams, &base_dir, &camera_id, &segment).await
                    }
                }),
            )
    }
}

async fn serve_playlist(
    state: &Arc<AppState>,
    streams: &Arc<RwLock<HashMap<String, StreamState>>>,
    base_dir: &PathBuf,
    camera_id: &str,
) -> impl IntoResponse {
    let server = StreamServer {
        state: state.clone(),
        streams: streams.clone(),
        base_dir: base_dir.clone(),
        idle_timeout_secs: 60,
    };

    match server.get_or_create_stream(camera_id) {
        Ok(output_dir) => {
            let path = output_dir.join("playlist.m3u8");
            // GStreamer hlssink2 may need a moment to write the first playlist; retry briefly
            let content = read_file_with_retry(&path, 20, std::time::Duration::from_millis(250)).await;
            match content {
                Ok(content) => (
                    StatusCode::OK,
                    [("Content-Type", "application/vnd.apple.mpegurl")],
                    content,
                )
                    .into_response(),
                Err(e) => {
                    warn!(path = ?path, error = %e, "Failed to read playlist");
                    (StatusCode::INTERNAL_SERVER_ERROR, "Stream not ready").into_response()
                }
            }
        }
        Err(e) => {
            warn!(camera_id = %camera_id, error = %e, "Failed to get stream");
            (StatusCode::NOT_FOUND, e).into_response()
        }
    }
}

/// Read file with retries (for playlist/segments that are created asynchronously by GStreamer)
async fn read_file_with_retry(
    path: &PathBuf,
    max_attempts: u32,
    delay: std::time::Duration,
) -> Result<String, std::io::Error> {
    for attempt in 1..=max_attempts {
        match fs::read_to_string(path).await {
            Ok(s) if !s.trim().is_empty() => return Ok(s),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        if attempt < max_attempts {
            tokio::time::sleep(delay).await;
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "file not ready in time",
    ))
}

async fn serve_segment(
    state: &Arc<AppState>,
    streams: &Arc<RwLock<HashMap<String, StreamState>>>,
    base_dir: &PathBuf,
    camera_id: &str,
    segment: &str,
) -> impl IntoResponse {
    let server = StreamServer {
        state: state.clone(),
        streams: streams.clone(),
        base_dir: base_dir.clone(),
        idle_timeout_secs: 60,
    };

    // Ensure stream is running
    if server.get_or_create_stream(camera_id).is_err() {
        return (StatusCode::NOT_FOUND, vec![]).into_response();
    }

    let path = base_dir.join(camera_id).join(segment);
    if !path.starts_with(base_dir) {
        return (StatusCode::FORBIDDEN, vec![]).into_response();
    }

    match fs::read(&path).await {
        Ok(data) => (
            StatusCode::OK,
            [("Content-Type", "video/MP2T")],
            data,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, vec![]).into_response(),
    }
}
