//! Camera MJPEG stream proxy
//!
//! Proxies the live MJPEG stream from the detector service to the web client.
//! Uses chunked streaming (no buffering) to preserve low latency (~200ms).
//!
//! Endpoint: GET /cameras/:id/stream/mjpeg
//!
//! Auth: JWT via Authorization: Bearer header OR ?token= query param.
//! The query-param fallback is required because browser fetch() with auth headers
//! is used in CameraStreamModal to work around <img> header limitations.

use std::sync::Arc;
use axum::{
    extract::{Path, Query},
    http::{header, HeaderMap},
    response::IntoResponse,
    routing::get,
    Extension, Router,
};
use chrono::Utc;
use futures_util::StreamExt as _;
use serde::Deserialize;
use uuid::Uuid;

use crate::{auth::verify_token, error::ApiError, AppState};

/// Stream proxy routes — merged with cameras router
pub fn routes() -> Router {
    Router::new().route("/:id/stream/mjpeg", get(proxy_mjpeg))
}

/// Query parameters for stream auth
#[derive(Deserialize)]
struct StreamQuery {
    /// JWT token as URL query param fallback when headers can't be set
    token: Option<String>,
}

/// Proxy the live MJPEG stream from detector to the browser client
async fn proxy_mjpeg(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<StreamQuery>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    // ── Auth: Authorization header first, then ?token= query param ───────────
    let token_str = {
        let header_token = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|s| s.to_string());

        if let Some(t) = header_token {
            t
        } else if let Some(t) = query.token {
            t
        } else {
            return Err(ApiError::AuthError(
                "Auth required: provide Authorization header or ?token= query param".to_string(),
            ));
        }
    };

    let token_data = verify_token(&token_str, &state.config.auth.jwt_secret)?;
    if token_data.claims.exp < Utc::now().timestamp() {
        return Err(ApiError::AuthError("Token expired".to_string()));
    }

    // ── Resolve detector_camera_id ────────────────────────────────────────────
    let camera = state
        .db
        .get_camera(&id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Camera {} not found", id)))?;

    let detector_camera_id = camera
        .detector_camera_id
        .unwrap_or_else(|| id.to_string());

    let stream_port = state.config.detector.stream_port();
    let url = format!(
        "http://{}:{}/stream/{}/mjpeg",
        state.config.detector.host, stream_port, detector_camera_id
    );

    // ── Open streaming connection ─────────────────────────────────────────────
    // Override the client's 10s timeout: MJPEG streams are infinite.
    // Using 24h so nginx's proxy_read_timeout (3600s) disconnects clients first.
    let response = state
        .http_client
        .get(&url)
        .timeout(std::time::Duration::from_secs(86400))
        .send()
        .await
        .map_err(|e| ApiError::InternalError(format!("Stream connect failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(ApiError::InternalError(format!(
            "Detector stream returned {}",
            response.status()
        )));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("multipart/x-mixed-replace;boundary=frame")
        .to_string();

    // Forward byte stream chunk-by-chunk — no buffering, preserves low latency
    let byte_stream = response.bytes_stream().map(|result| {
        result.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    });

    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "no-cache, no-store".to_string()),
        ],
        axum::body::Body::from_stream(byte_stream),
    ))
}
