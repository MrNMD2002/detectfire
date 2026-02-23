//! Health check route

use std::sync::Arc;
use axum::{Extension, Json};

use crate::{models::HealthResponse, AppState};

static START_TIME: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// Health check endpoint
pub async fn health_check(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<HealthResponse> {
    let start = START_TIME.get_or_init(std::time::Instant::now);
    let uptime = start.elapsed().as_secs();
    
    // Check database - use a simple query through db wrapper
    let db_status = match state.db.list_cameras().await {
        Ok(_) => "connected",
        Err(_) => "disconnected",
    };
    
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime as i64,
        database: db_status.to_string(),
        detector: "unknown".to_string(), // Would check gRPC connection
    })
}
