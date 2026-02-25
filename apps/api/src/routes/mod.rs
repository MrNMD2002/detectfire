//! API Routes

pub mod cameras;
pub mod events;
pub mod auth;
pub mod health;
pub mod stream;
pub mod snapshots;
pub mod settings;

use axum::{routing::get, Router};

/// Build API routes
pub fn api_routes() -> Router {
    Router::new()
        .nest("/auth", auth::routes())
        .nest("/cameras", cameras::routes().merge(stream::routes()))
        .nest("/events", events::routes())
        .nest("/snapshots", snapshots::routes())
        .nest("/settings", settings::routes())
        .route("/health", get(health::health_check))
}

/// Build WebSocket routes
pub fn ws_routes() -> Router {
    Router::new()
        .route("/events", get(crate::ws::ws_handler))
}
