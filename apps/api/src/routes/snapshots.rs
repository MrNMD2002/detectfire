//! Snapshot serving — GET /api/snapshots/{camera_id}/{filename}

use std::sync::Arc;
use axum::{
    extract::Path,
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Extension, Router,
};

use crate::{auth::AuthUser, AppState};

pub fn routes() -> Router {
    Router::new().route("/{camera_id}/{filename}", get(serve_snapshot))
}

async fn serve_snapshot(
    Extension(state): Extension<Arc<AppState>>,
    Path((camera_id, filename)): Path<(String, String)>,
    _user: AuthUser,
) -> impl IntoResponse {
    // Validate components — allow only safe characters
    if !is_safe_component(&camera_id) || !is_safe_component(&filename) {
        return (StatusCode::BAD_REQUEST, "Invalid path").into_response();
    }
    if !filename.ends_with(".jpg") && !filename.ends_with(".jpeg") {
        return (StatusCode::BAD_REQUEST, "Only JPEG allowed").into_response();
    }

    let base = std::path::Path::new(&state.config.storage.snapshot_path);
    let path = base.join(&camera_id).join(&filename);

    // Canonicalize to prevent path traversal
    let canonical = match std::fs::canonicalize(&path) {
        Ok(p) => p,
        Err(_) => return (StatusCode::NOT_FOUND, "Not found").into_response(),
    };
    let base_canonical = match std::fs::canonicalize(base) {
        Ok(p) => p,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Config error").into_response(),
    };
    if !canonical.starts_with(&base_canonical) {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    }

    match tokio::fs::read(&canonical).await {
        Ok(data) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "public, max-age=3600"),
            ],
            data,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

fn is_safe_component(s: &str) -> bool {
    !s.is_empty()
        && !s.contains("..")
        && !s.starts_with('.')
        && s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
}
