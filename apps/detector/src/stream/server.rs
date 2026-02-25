//! MJPEG stream server (MBFS-Stream approach)
//!
//! Replaces the old HLS/hlssink2 pipeline with a zero-copy MJPEG push stream:
//!   - Subscribes to the per-camera broadcast::Receiver<Arc<Frame>>
//!   - Encodes each frame as JPEG on demand (~2-5ms/frame, software)
//!   - Streams as multipart/x-mixed-replace (MJPEG) — native browser support via <img>
//!
//! Endpoint:
//!   GET /stream/:camera_id/mjpeg
//!
//! Benefits vs HLS:
//!   - ~200ms latency instead of ~6s (no segment buffering)
//!   - Reuses already-decoded frames from inference pipeline (no extra RTSP connection)
//!   - No temp files on disk
//!   - Single pipeline per camera (vs HLS which needed a separate pipeline)

use std::sync::Arc;

use axum::{
    extract::Path,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use bytes::Bytes;
use image::ImageFormat;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;
use tracing::{debug, info, warn};

use crate::camera::Frame;
use crate::AppState;

pub struct StreamServer {
    state: Arc<AppState>,
}

impl StreamServer {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    pub fn router(&self) -> Router {
        let state = self.state.clone();
        Router::new().route(
            "/stream/:camera_id/mjpeg",
            get(move |Path(camera_id): Path<String>| {
                serve_mjpeg(state.clone(), camera_id)
            }),
        )
    }
}

/// MJPEG stream handler
///
/// Subscribes to the camera's frame broadcast, encodes each frame as JPEG,
/// and streams them as multipart/x-mixed-replace.
async fn serve_mjpeg(state: Arc<AppState>, camera_id: String) -> Response {
    if !is_safe_camera_id(&camera_id) {
        return (StatusCode::BAD_REQUEST, "Invalid camera_id").into_response();
    }

    // Subscribe to the existing inference pipeline's broadcast channel
    let rx = match state.camera_manager.subscribe_to_camera(&camera_id) {
        Some(rx) => rx,
        None => {
            warn!(camera_id = %camera_id, "MJPEG request for unknown/stopped camera");
            return (
                StatusCode::NOT_FOUND,
                format!("Camera '{}' not found or not streaming", camera_id),
            )
                .into_response();
        }
    };

    info!(camera_id = %camera_id, "MJPEG stream client connected");

    // BroadcastStream wraps the receiver as an async Stream.
    // Lagged frames (Err) are filtered out; stream ends when sender is dropped.
    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        let cam = camera_id.clone();
        let frame = result.ok()?; // Lagged/closed → None, skips stale frames
        match encode_jpeg_frame(&frame) {
            Ok(jpeg) => {
                // MJPEG boundary format (RFC 2046 multipart)
                let part_header = format!(
                    "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                    jpeg.len()
                );
                let mut buf = Vec::with_capacity(part_header.len() + jpeg.len() + 2);
                buf.extend_from_slice(part_header.as_bytes());
                buf.extend_from_slice(&jpeg);
                buf.extend_from_slice(b"\r\n");

                debug!(camera_id = %cam, bytes = buf.len(), "MJPEG frame sent");
                Some(Ok::<Bytes, std::io::Error>(Bytes::from(buf)))
            }
            Err(e) => {
                warn!(camera_id = %cam, error = %e, "JPEG encode failed, skipping frame");
                None
            }
        }
    });

    (
        [
            (
                header::CONTENT_TYPE,
                "multipart/x-mixed-replace;boundary=frame",
            ),
            (header::CACHE_CONTROL, "no-cache, no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        axum::body::Body::from_stream(stream),
    )
        .into_response()
}

/// Encode an RGB Frame as JPEG bytes (quality 80)
fn encode_jpeg_frame(frame: &Frame) -> anyhow::Result<Vec<u8>> {
    let img = frame.to_image()
        .ok_or_else(|| anyhow::anyhow!("Invalid frame dimensions {}x{}", frame.width, frame.height))?;

    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Jpeg)?;
    Ok(buf)
}

/// Validate camera_id is filesystem-safe (no path traversal)
fn is_safe_camera_id(s: &str) -> bool {
    !s.is_empty()
        && !s.contains("..")
        && !s.contains('/')
        && !s.contains('\\')
        && !s.contains('\0')
}
