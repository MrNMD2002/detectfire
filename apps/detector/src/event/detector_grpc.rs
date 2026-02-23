//! gRPC service implementation for detector
//!
//! Provides gRPC interface for API service to communicate with detector.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

use crate::AppState;
use crate::camera::StreamState;
use crate::event::FireEvent;

// Include the generated protobuf code
pub mod proto {
    tonic::include_proto!("detector");
}

use proto::{
    detector_service_server::DetectorService, CameraStatus, CameraStatusList, DetectionEvent,
    Empty, HealthResponse, ReloadResponse,
};

/// gRPC service for detector
pub struct DetectorGrpcService {
    state: Arc<AppState>,
    start_time: Instant,
}

impl DetectorGrpcService {
    /// Create a new gRPC service
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            start_time: Instant::now(),
        }
    }
}

#[tonic::async_trait]
impl DetectorService for DetectorGrpcService {
    async fn health_check(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<HealthResponse>, Status> {
        let statuses = self.state.camera_manager.get_all_statuses();
        let active = statuses
            .iter()
            .filter(|s| s.state == StreamState::Streaming)
            .count();

        let response = HealthResponse {
            healthy: active > 0,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: self.start_time.elapsed().as_secs() as i64,
            active_cameras: active as i32,
            avg_inference_ms: 0.0, // Would calculate from metrics
        };

        Ok(Response::new(response))
    }

    async fn get_camera_statuses(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<CameraStatusList>, Status> {
        let statuses = self.state.camera_manager.get_all_statuses();

        let cameras = statuses
            .into_iter()
            .map(|s| {
                use proto::camera_status::Status;
                let status_enum = match s.state {
                    StreamState::Unknown => Status::Unknown as i32,
                    StreamState::Connecting => Status::Connecting as i32,
                    StreamState::Connected => Status::Connected as i32,
                    StreamState::Streaming => Status::Streaming as i32,
                    StreamState::Reconnecting => Status::Reconnecting as i32,
                    StreamState::Failed => Status::Failed as i32,
                    StreamState::Disabled => Status::Disabled as i32,
                };

                CameraStatus {
                    camera_id: s.camera_id,
                    site_id: s.site_id,
                    name: s.name,
                    status: status_enum,
                    reconnect_count: s.reconnect_count as i32,
                    fps_in: s.fps_in,
                    fps_infer: s.fps_infer,
                    last_frame_timestamp: s.last_frame_ts as i64,
                    error_message: s.last_error.unwrap_or_default(),
                }
            })
            .collect();

        Ok(Response::new(CameraStatusList { cameras }))
    }

    async fn reload_config(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ReloadResponse>, Status> {
        // Config reload would require restarting camera workers
        // For now, return not implemented
        Ok(Response::new(ReloadResponse {
            success: false,
            message: "Hot reload not implemented yet".to_string(),
            cameras_loaded: 0,
        }))
    }

    type StreamEventsStream = Pin<Box<dyn Stream<Item = Result<DetectionEvent, Status>> + Send>>;

    async fn stream_events(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        let rx = self.state.event_publisher.subscribe();

        let stream = async_stream::try_stream! {
            let mut rx = rx;
            while let Ok(event) = rx.recv().await {
                yield convert_event(event);
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }
}

/// Convert FireEvent to protobuf DetectionEvent
fn convert_event(event: FireEvent) -> DetectionEvent {
    use proto::detection_event::EventType;

    let event_type = match event.event_type {
        crate::event::EventType::Fire => EventType::Fire as i32,
        crate::event::EventType::Smoke => EventType::Smoke as i32,
        crate::event::EventType::StreamDown => EventType::StreamDown as i32,
        crate::event::EventType::StreamUp => EventType::StreamUp as i32,
    };

    DetectionEvent {
        event_id: event.event_id.to_string(),
        event_type,
        camera_id: event.camera_id,
        site_id: event.site_id,
        timestamp: event.timestamp.timestamp_millis(),
        confidence: event.confidence,
        detections: event
            .detections
            .into_iter()
            .map(|d| proto::Detection {
                class_name: d.class_name,
                confidence: d.confidence,
                bbox: Some(proto::BoundingBox {
                    x: d.bbox.x,
                    y: d.bbox.y,
                    width: d.bbox.width,
                    height: d.bbox.height,
                }),
            })
            .collect(),
        snapshot: event.snapshot_data.unwrap_or_default(),
        fps_in: event.metadata.fps_in.unwrap_or(0.0),
        fps_infer: event.metadata.fps_infer.unwrap_or(0.0),
        inference_ms: event.metadata.inference_ms.unwrap_or(0.0),
    }
}
