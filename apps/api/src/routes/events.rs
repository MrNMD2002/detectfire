//! Event routes

use std::sync::Arc;
use axum::{
    extract::{Path, Query},
    routing::{get, post},
    Extension, Json, Router,
};
use uuid::Uuid;

use serde::Serialize;

use crate::{
    auth::AuthUser,
    db::EventStats,
    error::ApiError,
    models::{Event, EventFilter},
    AppState,
};

/// Event routes
pub fn routes() -> Router {
    Router::new()
        .route("/", get(list_events))
        .route("/count", get(count_events))
        .route("/stats", get(event_stats))
        .route("/:id", get(get_event))
        .route("/:id/acknowledge", post(acknowledge_event))
}

#[derive(Serialize)]
struct EventsPage {
    data: Vec<Event>,
    total: i64,
}

/// List events with filters — returns data + total count for pagination
async fn list_events(
    Extension(state): Extension<Arc<AppState>>,
    Query(filter): Query<EventFilter>,
    _user: AuthUser,
) -> Result<Json<EventsPage>, ApiError> {
    let (data, total) = tokio::try_join!(
        state.db.list_events(&filter),
        state.db.count_events(&filter),
    )?;
    Ok(Json(EventsPage { data, total }))
}

/// Count events matching a filter (lightweight query for pagination)
async fn count_events(
    Extension(state): Extension<Arc<AppState>>,
    Query(filter): Query<EventFilter>,
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let total = state.db.count_events(&filter).await?;
    Ok(Json(serde_json::json!({ "total": total })))
}

/// Get event by ID
async fn get_event(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
) -> Result<Json<Event>, ApiError> {
    // Fix: query by event_id, not camera_id
    let event = state.db.get_event_by_id(&id).await?
        .ok_or_else(|| ApiError::NotFound(format!("Event {} not found", id)))?;

    Ok(Json(event))
}

/// Acknowledge an event
async fn acknowledge_event(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<Uuid>,
    user: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let updated = state.db.acknowledge_event(&id, &user.id).await?;

    if !updated {
        return Err(ApiError::NotFound(format!("Event {} not found", id)));
    }

    Ok(Json(serde_json::json!({
        "acknowledged": true,
        "acknowledged_by": user.id,
        "acknowledged_at": chrono::Utc::now()
    })))
}

/// Event statistics — computed in the database, not in memory
async fn event_stats(
    Extension(state): Extension<Arc<AppState>>,
    Query(filter): Query<EventStatsFilter>,
    _user: AuthUser,
) -> Result<Json<EventStats>, ApiError> {
    let stats = state.db.event_stats(filter.start_time, filter.end_time).await?;
    Ok(Json(stats))
}

#[derive(Debug, serde::Deserialize)]
struct EventStatsFilter {
    start_time: Option<chrono::DateTime<chrono::Utc>>,
    end_time: Option<chrono::DateTime<chrono::Utc>>,
}
