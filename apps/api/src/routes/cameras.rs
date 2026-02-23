//! Camera routes

use std::sync::Arc;
use axum::{
    extract::Path,
    routing::{delete, get, post, put},
    Extension, Json, Router,
};
use uuid::Uuid;
use validator::Validate;

use crate::{
    auth::AuthUser,
    detector_client::DetectorClient,
    error::ApiError,
    models::{Camera, CreateCameraInput, UpdateCameraInput},
    AppState,
};
use tracing::warn;

/// Camera routes
pub fn routes() -> Router {
    Router::new()
        .route("/", get(list_cameras))
        .route("/", post(create_camera))
        .route("/:id", get(get_camera))
        .route("/:id", put(update_camera))
        .route("/:id", delete(delete_camera))
        .route("/:id/status", get(get_camera_status))
}

/// List all cameras
async fn list_cameras(
    Extension(state): Extension<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<Vec<Camera>>, ApiError> {
    let cameras = state.db.list_cameras().await?;
    Ok(Json(cameras))
}

/// Get camera by ID
async fn get_camera(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
) -> Result<Json<Camera>, ApiError> {
    let camera = state.db.get_camera(&id).await?
        .ok_or_else(|| ApiError::NotFound(format!("Camera {} not found", id)))?;
    
    Ok(Json(camera))
}

/// Create new camera
async fn create_camera(
    Extension(state): Extension<Arc<AppState>>,
    _user: AuthUser,
    Json(input): Json<CreateCameraInput>,
) -> Result<Json<Camera>, ApiError> {
    input.validate()?;
    
    let camera = state.db.create_camera(&input).await?;
    
    // Notify detector to reload config
    if let Err(e) = notify_detector_reload(&state).await {
        warn!(error = %e, "Failed to notify detector to reload config");
        // Don't fail the request, just log the warning
    }
    
    Ok(Json(camera))
}

/// Update camera
async fn update_camera(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
    Json(input): Json<UpdateCameraInput>,
) -> Result<Json<Camera>, ApiError> {
    input.validate()?;
    
    let camera = state.db.update_camera(&id, &input).await?
        .ok_or_else(|| ApiError::NotFound(format!("Camera {} not found", id)))?;
    
    // Notify detector to reload config
    if let Err(e) = notify_detector_reload(&state).await {
        warn!(error = %e, "Failed to notify detector to reload config");
        // Don't fail the request, just log the warning
    }
    
    Ok(Json(camera))
}

/// Delete camera
async fn delete_camera(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _camera = state.db.get_camera(&id).await?;
    let deleted = state.db.delete_camera(&id).await?;
    
    if !deleted {
        return Err(ApiError::NotFound(format!("Camera {} not found", id)));
    }
    
    // Notify detector to reload config (will stop the deleted camera)
    if let Err(e) = notify_detector_reload(&state).await {
        warn!(error = %e, "Failed to notify detector to reload config");
        // Don't fail the request, just log the warning
    }
    
    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// Get camera status from detector
async fn get_camera_status(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Get camera to find detector_camera_id
    let camera = state.db.get_camera(&id).await?
        .ok_or_else(|| ApiError::NotFound(format!("Camera {} not found", id)))?;
    
    // Create owned String for detector_camera_id to avoid borrow checker issues
    let detector_camera_id = camera.detector_camera_id
        .unwrap_or_else(|| id.to_string());
    
    // Query detector via gRPC
    let detector_addr = format!(
        "{}:{}",
        state.config.detector.host,
        state.config.detector.grpc_port
    );
    
    match DetectorClient::connect(&detector_addr).await {
        Ok(mut client) => {
            match client.get_camera_status(&detector_camera_id).await {
                Ok(Some(status)) => {
                    let status_str = match status.status {
                        0 => "unknown",
                        1 => "connecting",
                        2 => "connected",
                        3 => "streaming",
                        4 => "reconnecting",
                        5 => "failed",
                        6 => "disabled",
                        _ => "unknown",
                    };
                    
                    Ok(Json(serde_json::json!({
                        "camera_id": id,
                        "detector_camera_id": &detector_camera_id,
                        "status": status_str,
                        "reconnect_count": status.reconnect_count,
                        "fps_in": status.fps_in,
                        "fps_infer": status.fps_infer,
                        "last_frame_timestamp": status.last_frame_timestamp,
                        "error_message": status.error_message,
                    })))
                }
                Ok(None) => {
                    Ok(Json(serde_json::json!({
                        "camera_id": id,
                        "detector_camera_id": &detector_camera_id,
                        "status": "not_found",
                        "message": "Camera not found in detector"
                    })))
                }
                Err(e) => {
                    warn!(error = %e, "Failed to get camera status from detector");
                    Ok(Json(serde_json::json!({
                        "camera_id": id,
                        "status": "error",
                        "message": format!("Failed to query detector: {}", e)
                    })))
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "Failed to connect to detector");
            Ok(Json(serde_json::json!({
                "camera_id": id,
                "status": "detector_unavailable",
                "message": format!("Detector connection failed: {}", e)
            })))
        }
    }
}

/// Helper function to notify detector to reload config
async fn notify_detector_reload(state: &AppState) -> anyhow::Result<()> {
    let detector_addr = format!(
        "{}:{}",
        state.config.detector.host,
        state.config.detector.grpc_port
    );
    
    let mut client = DetectorClient::connect(&detector_addr).await?;
    let response = client.reload_config().await?;
    
    if !response.success {
        return Err(anyhow::anyhow!("Detector reload failed: {}", response.message));
    }
    
    Ok(())
}
