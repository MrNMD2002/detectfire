//! Event publisher
//!
//! Publishes events to API service and handles snapshot saving.

use std::path::PathBuf;
use std::sync::Arc;
use chrono::Utc;
use image::codecs::jpeg::JpegEncoder;
use parking_lot::RwLock;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::camera::Frame;
use crate::config::{GlobalConfig, SnapshotConfig};
use crate::decision::{DecisionEvent, DecisionEventType};
use crate::error::DetectorResult;

use super::models::{EventMetadata, EventType, FireEvent};

/// Event publisher for broadcasting events
#[derive(Clone)]
pub struct EventPublisher {
    inner: Arc<EventPublisherInner>,
}

struct EventPublisherInner {
    /// Broadcast channel for events
    tx: broadcast::Sender<FireEvent>,
    
    /// Snapshot configuration
    snapshot_config: SnapshotConfig,
    
    /// Base path for snapshots
    snapshot_base_path: PathBuf,
    
    /// Event counter
    event_count: RwLock<u64>,
}

/// Default capacity for the event broadcast channel.
/// Increase if downstream gRPC consumers cannot keep up.
const DEFAULT_BROADCAST_CAPACITY: usize = 1_024;

impl EventPublisher {
    /// Create a new event publisher
    pub fn new(config: &GlobalConfig) -> DetectorResult<Self> {
        let capacity = std::env::var("DETECTOR__EVENT_BROADCAST_CAPACITY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(DEFAULT_BROADCAST_CAPACITY);

        let (tx, _) = broadcast::channel(capacity);
        
        let snapshot_base_path = PathBuf::from(&config.storage.snapshots.path);
        
        // Create snapshot directory if enabled – propagate errors so misconfiguration
        // is caught at startup rather than silently losing snapshots at runtime.
        if config.storage.snapshots.enabled {
            std::fs::create_dir_all(&snapshot_base_path).map_err(|e| {
                crate::error::DetectorError::SnapshotError(format!(
                    "Failed to create snapshot directory '{}': {}",
                    snapshot_base_path.display(),
                    e
                ))
            })?;
        }
        
        Ok(Self {
            inner: Arc::new(EventPublisherInner {
                tx,
                snapshot_config: config.storage.snapshots.clone(),
                snapshot_base_path,
                event_count: RwLock::new(0),
            }),
        })
    }
    
    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<FireEvent> {
        self.inner.tx.subscribe()
    }
    
    /// Publish a detection event
    pub async fn publish(&self, decision: DecisionEvent, frame: &Frame) -> DetectorResult<()> {
        let event_type = match decision.event_type {
            DecisionEventType::Fire => EventType::Fire,
            DecisionEventType::Smoke => EventType::Smoke,
        };
        
        // Create base event
        let mut event = FireEvent::new(
            event_type,
            &decision.camera_id,
            &decision.site_id,
            decision.confidence,
            decision.detections,
        );
        
        // Save snapshot if enabled
        if self.inner.snapshot_config.enabled {
            match self.save_snapshot(frame, &decision.camera_id).await {
                Ok((path, data)) => {
                    event.snapshot_path = Some(path);
                    event.snapshot_data = Some(data);
                }
                Err(e) => {
                    warn!(
                        camera_id = %decision.camera_id,
                        error = %e,
                        "Failed to save snapshot"
                    );
                }
            }
        }
        
        // Add metadata
        event.metadata = EventMetadata {
            frame_width: Some(frame.width),
            frame_height: Some(frame.height),
            ..Default::default()
        };
        
        // Broadcast event
        self.broadcast_event(event);
        
        Ok(())
    }
    
    /// Publish stream down event
    pub async fn publish_stream_down(&self, camera_id: &str, error: &str) -> DetectorResult<()> {
        // We need site_id - for now use empty, API will fill in
        let event = FireEvent::stream_down(camera_id, "", error);
        self.broadcast_event(event);
        Ok(())
    }
    
    /// Publish stream up event
    pub async fn publish_stream_up(&self, camera_id: &str) -> DetectorResult<()> {
        let event = FireEvent::stream_up(camera_id, "");
        self.broadcast_event(event);
        Ok(())
    }
    
    /// Save snapshot and return (path, jpeg_bytes)
    async fn save_snapshot(
        &self,
        frame: &Frame,
        camera_id: &str,
    ) -> DetectorResult<(String, Vec<u8>)> {
        // Convert frame to image
        let img = frame.to_image()
            .ok_or_else(|| crate::error::DetectorError::SnapshotError(
                "Failed to convert frame to image".to_string()
            ))?;
        
        // Encode as JPEG
        let mut jpeg_bytes = Vec::new();
        {
            let mut encoder = JpegEncoder::new_with_quality(
                &mut jpeg_bytes,
                self.inner.snapshot_config.quality,
            );
            
            encoder.encode_image(&img)
                .map_err(|e| crate::error::DetectorError::SnapshotError(e.to_string()))?;
        }
        
        // Generate filename
        let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%S%.3f");
        let filename = format!("{}_{}.jpg", camera_id, timestamp);
        
        // Create camera subdirectory – log a warning on failure; snapshot will still
        // fail at write time, but we want an explicit message for diagnostics.
        let camera_dir = self.inner.snapshot_base_path.join(camera_id);
        if let Err(e) = std::fs::create_dir_all(&camera_dir) {
            warn!(
                camera_id = %camera_id,
                path = %camera_dir.display(),
                error = %e,
                "Failed to create camera snapshot directory"
            );
        }
        
        // Save file
        let filepath = camera_dir.join(&filename);
        tokio::fs::write(&filepath, &jpeg_bytes).await
            .map_err(|e| crate::error::DetectorError::SnapshotError(e.to_string()))?;
        
        // Return relative path for database
        let relative_path = format!("{}/{}", camera_id, filename);
        
        debug!(
            camera_id = %camera_id,
            path = %relative_path,
            size_kb = jpeg_bytes.len() / 1024,
            "Snapshot saved"
        );
        
        Ok((relative_path, jpeg_bytes))
    }
    
    /// Broadcast event to all subscribers
    fn broadcast_event(&self, event: FireEvent) {
        // Increment counter
        *self.inner.event_count.write() += 1;
        
        // Log event
        info!(
            event_id = %event.event_id,
            event_type = %event.event_type,
            camera_id = %event.camera_id,
            site_id = %event.site_id,
            confidence = event.confidence,
            detections = event.detections.len(),
            "Publishing event"
        );
        
        // Broadcast
        if self.inner.tx.receiver_count() > 0 {
            if let Err(e) = self.inner.tx.send(event) {
                error!(error = %e, "Failed to broadcast event");
            }
        } else {
            debug!("No subscribers, event not broadcast");
        }
    }
    
    /// Get total event count
    pub fn event_count(&self) -> u64 {
        *self.inner.event_count.read()
    }
    
    /// Get subscriber count
    pub fn subscriber_count(&self) -> usize {
        self.inner.tx.receiver_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;

    fn make_test_config() -> GlobalConfig {
        GlobalConfig {
            server: ServerConfig {
                api: ApiServerConfig {
                    host: "0.0.0.0".to_string(),
                    port: 8080,
                    workers: 4,
                },
                detector: DetectorServerConfig {
                    host: "0.0.0.0".to_string(),
                    grpc_port: 50051,
                },
            },
            inference: InferenceConfig::default(),
            reconnect: ReconnectConfig::default(),
            telegram: TelegramConfig {
                enabled: false,
                bot_token: "".to_string(),
                default_chat_id: "".to_string(),
                rate_limit: TelegramRateLimit::default(),
                templates: TelegramTemplates::default(),
            },
            logging: LoggingConfig::default(),
            storage: StorageConfig {
                snapshots: SnapshotConfig {
                    enabled: false,
                    ..Default::default()
                },
                minio: MinioConfig::default(),
            },
            monitoring: MonitoringConfig::default(),
        }
    }

    #[tokio::test]
    async fn test_event_publisher_creation() {
        let config = make_test_config();
        let publisher = EventPublisher::new(&config).unwrap();
        
        assert_eq!(publisher.event_count(), 0);
        assert_eq!(publisher.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn test_event_publisher_subscribe() {
        let config = make_test_config();
        let publisher = EventPublisher::new(&config).unwrap();
        
        let _rx = publisher.subscribe();
        assert_eq!(publisher.subscriber_count(), 1);
    }
}
