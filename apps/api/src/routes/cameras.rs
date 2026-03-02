//! Camera routes

use std::collections::HashMap;
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
    camera_sync,
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
        .route("/statuses", get(get_all_camera_statuses))
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

/// Get status for all cameras from detector in a single gRPC call.
/// Returns a map of { camera_uuid: CameraStatusData } for O(1) frontend lookup.
async fn get_all_camera_statuses(
    Extension(state): Extension<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cameras = state.db.list_cameras().await?;

    let detector_addr = format!(
        "{}:{}",
        state.config.detector.host,
        state.config.detector.grpc_port
    );

    // Single gRPC call for all statuses
    let (detector_available, status_by_detector_id): (bool, HashMap<String, serde_json::Value>) =
        match DetectorClient::connect(&detector_addr).await {
            Ok(mut client) => match client.get_camera_statuses().await {
                Ok(status_list) => {
                    let map = status_list
                        .cameras
                        .into_iter()
                        .map(|s| {
                            let status_str = match s.status {
                                0 => "unknown",
                                1 => "connecting",
                                2 => "connected",
                                3 => "streaming",
                                4 => "reconnecting",
                                5 => "failed",
                                6 => "disabled",
                                _ => "unknown",
                            };
                            let val = serde_json::json!({
                                "status": status_str,
                                "reconnect_count": s.reconnect_count,
                                "fps_in": s.fps_in,
                                "fps_infer": s.fps_infer,
                                "last_frame_timestamp": s.last_frame_timestamp,
                                "error_message": s.error_message,
                            });
                            (s.camera_id, val)
                        })
                        .collect();
                    (true, map)
                }
                Err(e) => {
                    warn!(error = %e, "Failed to get camera statuses from detector");
                    (false, HashMap::new())
                }
            },
            Err(e) => {
                warn!(error = %e, "Failed to connect to detector");
                (false, HashMap::new())
            }
        };

    // Build result map keyed by camera UUID
    let mut result = serde_json::Map::new();
    for camera in &cameras {
        let detector_cam_id = camera
            .detector_camera_id
            .clone()
            .unwrap_or_else(|| camera.id.to_string());

        let mut status = if let Some(s) = status_by_detector_id.get(&detector_cam_id) {
            s.clone()
        } else {
            let status_str = if detector_available { "not_found" } else { "detector_unavailable" };
            serde_json::json!({ "status": status_str })
        };

        if let serde_json::Value::Object(ref mut m) = status {
            m.insert("detector_camera_id".to_string(), serde_json::json!(detector_cam_id));
        }

        result.insert(camera.id.to_string(), status);
    }

    Ok(Json(serde_json::Value::Object(result)))
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

/// Sync cameras to cameras.yaml and tell the detector to reload.
/// Called after every create/update/delete camera operation.
async fn notify_detector_reload(state: &AppState) -> anyhow::Result<()> {
    // Fetch all cameras (with decrypted RTSP URLs) and write cameras.yaml
    let cameras = state.db.list_cameras().await?;
    if let Err(e) = camera_sync::write_cameras_yaml(&cameras) {
        // Not fatal — detector can still use old file; log and continue
        warn!(error = %e, "Failed to write cameras.yaml; detector reload may use stale config");
    }

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
