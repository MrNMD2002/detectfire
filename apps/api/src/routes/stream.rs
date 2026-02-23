//! Camera HLS stream proxy
//!
//! Proxies stream requests from web to detector service.

use std::sync::Arc;
use axum::{
    extract::Path,
    http::header,
    response::IntoResponse,
    routing::get,
    Extension, Router,
};
use uuid::Uuid;

use crate::{auth::AuthUser, error::ApiError, AppState};

/// Stream proxy routes - merge with cameras router
pub fn routes() -> Router {
    Router::new()
        .route("/:id/stream/playlist.m3u8", get(proxy_playlist))
        .route("/:id/stream/:segment", get(proxy_segment))
}

/// Proxy HLS playlist request to detector
async fn proxy_playlist(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
) -> Result<impl IntoResponse, ApiError> {
    proxy_stream_file(&state, id, "playlist.m3u8", "application/vnd.apple.mpegurl").await
}

/// Proxy HLS segment request to detector
async fn proxy_segment(
    Extension(state): Extension<Arc<AppState>>,
    Path((id, segment)): Path<(Uuid, String)>,
    _user: AuthUser,
) -> Result<impl IntoResponse, ApiError> {
    proxy_stream_file(&state, id, &segment, "video/MP2T").await
}

async fn proxy_stream_file(
    state: &AppState,
    camera_id: Uuid,
    filename: &str,
    content_type: &str,
) -> Result<impl IntoResponse, ApiError> {
    let camera = state
        .db
        .get_camera(&camera_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Camera {} not found", camera_id)))?;

    // Lưu vào String để tránh lifetime issue
    let detector_camera_id = camera
        .detector_camera_id
        .unwrap_or_else(|| camera_id.to_string());
    
    // Clone content_type thành String để tránh lifetime issue
    let content_type_owned = content_type.to_string();

    let stream_port = state.config.detector.stream_port();
    let url = format!(
        "http://{}:{}/stream/{}/{}",
        state.config.detector.host,
        stream_port,
        detector_camera_id,
        filename
    );

    // Use the shared HTTP client from AppState rather than creating one per request
    let response = state.http_client
        .get(&url)
        .send()
        .await
        .map_err(|e| ApiError::InternalError(format!("Stream request failed: {}", e)))?;

    let status_code = response.status();
    if !status_code.is_success() {
        return Err(ApiError::InternalError(format!(
            "Detector stream returned {}",
            status_code
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| ApiError::InternalError(format!("Stream read failed: {}", e)))?;

    // For playlist.m3u8, rewrite URLs to point back through API proxy
    let body = if filename.ends_with(".m3u8") {
        let text = String::from_utf8_lossy(&bytes);
        let rewritten = text.replace(
            &format!("/stream/{}/", detector_camera_id),
            &format!("/api/cameras/{}/stream/", camera_id),
        );
        rewritten.into_bytes()
    } else {
        bytes.to_vec()
    };

    // Convert reqwest status to axum StatusCode
    let status = axum::http::StatusCode::from_u16(status_code.as_u16())
        .unwrap_or(axum::http::StatusCode::OK);

    Ok((
        status,
        [(header::CONTENT_TYPE, content_type_owned)],
        body,
    ))
}
