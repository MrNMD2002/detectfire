//! gRPC client for connecting to detector service

use std::sync::Arc;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use tonic::transport::Channel;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    models::CreateEventInput,
    AppState,
};

// Include generated proto code
pub mod proto {
    tonic::include_proto!("detector");
}

use proto::{
    detector_service_client::DetectorServiceClient,
    DetectionEvent,
    Empty,
    BoundingBox,
    CameraStatusList,
    ReloadResponse,
};

/// gRPC client for detector service
pub struct DetectorClient {
    client: DetectorServiceClient<Channel>,
}

impl DetectorClient {
    /// Create a new detector client
    pub async fn connect(addr: &str) -> Result<Self> {
        info!(addr = %addr, "Connecting to detector service");
        
        let channel = Channel::from_shared(format!("http://{}", addr))
            .context("Invalid detector address")?
            .connect()
            .await
            .context("Failed to connect to detector service")?;
        
        let client = DetectorServiceClient::new(channel);
        
        info!("Connected to detector service");
        
        Ok(Self { client })
    }
    
    /// Stream events from detector
    pub async fn stream_events(
        &mut self,
        state: Arc<AppState>,
    ) -> Result<()> {
        info!("Starting event stream from detector");
        
        // Create request
        let request = tonic::Request::new(Empty {});
        
        // Call StreamEvents RPC
        let mut stream = self
            .client
            .stream_events(request)
            .await
            .context("Failed to call StreamEvents RPC")?
            .into_inner();
        
        info!("Event stream established, receiving events...");
        
        // Process events from stream
        while let Some(event_result) = stream.message().await.transpose() {
            match event_result {
                Ok(event) => {
                    if let Err(e) = Self::handle_event(&event, &state).await {
                        error!(error = %e, "Failed to handle event");
                        // Continue processing other events
                    }
                }
                Err(e) => {
                    error!(error = %e, "Error receiving event from stream");
                    // Break on stream error - will trigger reconnection
                    return Err(e.into());
                }
            }
        }
        
        warn!("Event stream ended");
        Ok(())
    }
    
    /// Reload detector configuration
    pub async fn reload_config(&mut self) -> Result<ReloadResponse> {
        let request = tonic::Request::new(Empty {});
        let response = self
            .client
            .reload_config(request)
            .await
            .context("Failed to call ReloadConfig RPC")?;
        
        Ok(response.into_inner())
    }
    
    /// Get all camera statuses from detector
    pub async fn get_camera_statuses(&mut self) -> Result<CameraStatusList> {
        let request = tonic::Request::new(Empty {});
        let response = self
            .client
            .get_camera_statuses(request)
            .await
            .context("Failed to call GetCameraStatuses RPC")?;
        
        Ok(response.into_inner())
    }
    
    /// Get status for a specific camera by detector_camera_id
    pub async fn get_camera_status(
        &mut self,
        detector_camera_id: &str,
    ) -> Result<Option<proto::CameraStatus>> {
        let statuses = self.get_camera_statuses().await?;
        Ok(statuses
            .cameras
            .into_iter()
            .find(|s| s.camera_id == detector_camera_id))
    }
    
    /// Handle a single event from detector (internal helper)
    async fn handle_event(
        proto_event: &DetectionEvent,
        state: &Arc<AppState>,
    ) -> Result<()> {
        // Convert proto event to internal format (resolve camera_id: detector uses "cam-01", API uses UUID)
        let create_input = convert_proto_event(proto_event, state).await?;
        
        // Save to database
        let saved_event = match state.db.save_event(&create_input).await {
            Ok(event) => {
                info!(
                    event_id = %event.id,
                    event_type = %event.event_type,
                    camera_id = %event.camera_id,
                    "Event saved to database"
                );
                event
            }
            Err(e) => {
                error!(error = %e, "Failed to save event to database");
                return Err(e.into());
            }
        };
        
        // Get camera info (reuse for both WebSocket and Telegram)
        let camera = state.db.get_camera(&saved_event.camera_id).await?;
        let camera_name = camera
            .as_ref()
            .map(|c| c.name.as_str())
            .unwrap_or("Unknown Camera");
        
        // Broadcast to WebSocket clients
        // Add camera_name to event for web UI
        let event_for_ws = serde_json::json!({
            "id": saved_event.id,
            "event_type": saved_event.event_type,
            "camera_id": saved_event.camera_id,
            "camera_name": camera_name,
            "site_id": saved_event.site_id,
            "timestamp": saved_event.timestamp,
            "confidence": saved_event.confidence,
            "detections": saved_event.detections,
            "snapshot_path": saved_event.snapshot_path,
            "metadata": saved_event.metadata,
            "acknowledged": saved_event.acknowledged,
            "acknowledged_by": saved_event.acknowledged_by,
            "acknowledged_at": saved_event.acknowledged_at,
        });
        
        let event_json = serde_json::to_string(&event_for_ws)
            .context("Failed to serialize event for WebSocket")?;
        state.event_broadcaster.broadcast(event_json);
        
        // Send Telegram notification for fire/smoke events
        use proto::detection_event::EventType;
        
        // Convert i32 to EventType enum
        let event_type_enum = match proto_event.event_type {
            0 => EventType::Fire,
            1 => EventType::Smoke,
            2 => EventType::StreamDown,
            3 => EventType::StreamUp,
            _ => EventType::Fire, // Default to Fire for unknown
        };
        
        if matches!(event_type_enum, EventType::Fire | EventType::Smoke) {
            
            let event_type_str = match event_type_enum {
                EventType::Fire => "fire",
                EventType::Smoke => "smoke",
                _ => return Ok(()),
            };
            
            // Send Telegram alert with snapshot if available
            if !proto_event.snapshot.is_empty() {
                if let Err(e) = state
                    .telegram
                    .send_fire_alert(
                        camera_name,
                        &saved_event.site_id,
                        event_type_str,
                        saved_event.confidence,
                        Some(proto_event.snapshot.clone()),
                    )
                    .await
                {
                    warn!(error = %e, "Failed to send Telegram alert");
                }
            } else {
                if let Err(e) = state
                    .telegram
                    .send_fire_alert(
                        camera_name,
                        &saved_event.site_id,
                        event_type_str,
                        saved_event.confidence,
                        None,
                    )
                    .await
                {
                    warn!(error = %e, "Failed to send Telegram alert");
                }
            }
        }
        
        Ok(())
    }
}

/// Convert protobuf DetectionEvent to internal CreateEventInput
async fn convert_proto_event(
    proto_event: &DetectionEvent,
    state: &Arc<AppState>,
) -> Result<CreateEventInput> {
    use proto::detection_event::EventType;
    
    // Convert i32 to EventType enum
    let event_type_enum = match proto_event.event_type {
        0 => EventType::Fire,
        1 => EventType::Smoke,
        2 => EventType::StreamDown,
        3 => EventType::StreamUp,
        _ => EventType::Fire, // Default to Fire for unknown
    };
    
    let event_type = match event_type_enum {
        EventType::Fire => "fire",
        EventType::Smoke => "smoke",
        EventType::StreamDown => "stream_down",
        EventType::StreamUp => "stream_up",
    };
    
    // Convert detections to JSON
    let detections_json = serde_json::json!(
        proto_event.detections.iter().map(|d| {
            let bbox = d.bbox.as_ref().unwrap_or(&BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            });
            serde_json::json!({
                "class": d.class_name.clone(),
                "confidence": d.confidence,
                "bbox": {
                    "x": bbox.x,
                    "y": bbox.y,
                    "width": bbox.width,
                    "height": bbox.height,
                }
            })
        }).collect::<Vec<_>>()
    );
    
    // Convert metadata
    let metadata_json = serde_json::json!({
        "fps_in": proto_event.fps_in,
        "fps_infer": proto_event.fps_infer,
        "inference_ms": proto_event.inference_ms,
    });
    
    // Resolve camera_id: detector sends "cam-01", API uses UUID
    let camera_uuid = match Uuid::parse_str(&proto_event.camera_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            // Not a UUID - look up by detector_camera_id
            state
                .db
                .get_camera_by_detector_id(&proto_event.camera_id)
                .await?
                .map(|c| c.id)
                .ok_or_else(|| anyhow::anyhow!(
                    "No API camera with detector_camera_id = '{}'",
                    proto_event.camera_id
                ))?
        }
    };

    Ok(CreateEventInput {
        event_type: event_type.to_string(),
        camera_id: camera_uuid,
        site_id: proto_event.site_id.clone(),
        timestamp: DateTime::from_timestamp_millis(proto_event.timestamp)
            .unwrap_or_else(Utc::now),
        confidence: proto_event.confidence,
        detections: detections_json,
        snapshot_path: None, // Will be set if snapshot is saved to disk
        metadata: metadata_json,
    })
}
