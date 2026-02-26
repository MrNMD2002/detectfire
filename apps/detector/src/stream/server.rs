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
use image::codecs::jpeg::JpegEncoder;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;
use tracing::{info, warn};

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
        let frame = match result {
            Ok(f) => f,
            Err(_lagged) => return None, // Lagged/closed → skip stale frames silently
        };
        match encode_jpeg_frame(&frame) {
            Ok(jpeg) => {
                // MJPEG boundary format (RFC 2046 multipart)
                // Pre-allocate exact size: header + jpeg + trailing CRLF
                let part_header = format!(
                    "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                    jpeg.len()
                );
                let total = part_header.len() + jpeg.len() + 2;
                let mut buf = Vec::with_capacity(total);
                buf.extend_from_slice(part_header.as_bytes());
                buf.extend_from_slice(&jpeg);
                buf.extend_from_slice(b"\r\n");

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

/// JPEG quality for MJPEG stream (75 = good quality/size tradeoff, ~2-5ms encode time)
const MJPEG_JPEG_QUALITY: u8 = 75;

/// Encode an RGB Frame as JPEG bytes.
///
/// Encodes directly from the raw Arc<Vec<u8>> pixel buffer — avoids the extra
/// Vec<u8> clone that image::RgbImage::from_raw() would require. Pre-sizes the
/// output buffer to ~25% of raw size (typical JPEG compression ratio for camera feeds).
fn encode_jpeg_frame(frame: &Frame) -> anyhow::Result<Vec<u8>> {
    if !frame.is_valid() {
        anyhow::bail!("Invalid frame {}x{} (data={})", frame.width, frame.height, frame.data.len());
    }

    // Pre-allocate ~25% of raw RGB size as a typical JPEG size estimate
    let capacity = (frame.width as usize * frame.height as usize * 3) / 4;
    let mut buf = Vec::with_capacity(capacity);

    let mut encoder = JpegEncoder::new_with_quality(&mut buf, MJPEG_JPEG_QUALITY);
    encoder.encode(
        &frame.data,
        frame.width,
        frame.height,
        image::ExtendedColorType::Rgb8,
    )?;

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
