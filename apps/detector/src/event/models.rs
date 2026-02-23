//! Event models
//!
//! Defines event types and structures for fire/smoke detection events.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::inference::{Detection, BoundingBox};

/// Event type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Fire,
    Smoke,
    StreamDown,
    StreamUp,
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fire => write!(f, "fire"),
            Self::Smoke => write!(f, "smoke"),
            Self::StreamDown => write!(f, "stream_down"),
            Self::StreamUp => write!(f, "stream_up"),
        }
    }
}

/// A fire/smoke detection event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FireEvent {
    /// Unique event ID
    pub event_id: Uuid,
    
    /// Event type
    pub event_type: EventType,
    
    /// Camera ID
    pub camera_id: String,
    
    /// Site ID
    pub site_id: String,
    
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    
    /// Overall confidence score
    pub confidence: f32,
    
    /// Individual detections
    pub detections: Vec<DetectionInfo>,
    
    /// Snapshot path (if saved)
    pub snapshot_path: Option<String>,
    
    /// Snapshot as JPEG bytes (for sending)
    #[serde(skip)]
    pub snapshot_data: Option<Vec<u8>>,
    
    /// Event metadata
    pub metadata: EventMetadata,
}

impl FireEvent {
    /// Create a new fire event
    pub fn fire(
        camera_id: &str,
        site_id: &str,
        confidence: f32,
        detections: Vec<Detection>,
    ) -> Self {
        Self::new(EventType::Fire, camera_id, site_id, confidence, detections)
    }
    
    /// Create a new smoke event
    pub fn smoke(
        camera_id: &str,
        site_id: &str,
        confidence: f32,
        detections: Vec<Detection>,
    ) -> Self {
        Self::new(EventType::Smoke, camera_id, site_id, confidence, detections)
    }
    
    /// Create a new stream down event
    pub fn stream_down(camera_id: &str, site_id: &str, error: &str) -> Self {
        let mut event = Self::new(EventType::StreamDown, camera_id, site_id, 0.0, vec![]);
        event.metadata.error_message = Some(error.to_string());
        event
    }
    
    /// Create a new stream up event
    pub fn stream_up(camera_id: &str, site_id: &str) -> Self {
        Self::new(EventType::StreamUp, camera_id, site_id, 0.0, vec![])
    }
    
    /// Create a new event
    pub fn new(
        event_type: EventType,
        camera_id: &str,
        site_id: &str,
        confidence: f32,
        detections: Vec<Detection>,
    ) -> Self {
        let detection_infos = detections
            .into_iter()
            .map(DetectionInfo::from)
            .collect();
        
        Self {
            event_id: Uuid::new_v4(),
            event_type,
            camera_id: camera_id.to_string(),
            site_id: site_id.to_string(),
            timestamp: Utc::now(),
            confidence,
            detections: detection_infos,
            snapshot_path: None,
            snapshot_data: None,
            metadata: EventMetadata::default(),
        }
    }
    
    /// Set snapshot data
    pub fn with_snapshot(mut self, data: Vec<u8>) -> Self {
        self.snapshot_data = Some(data);
        self
    }
    
    /// Set snapshot path
    pub fn with_snapshot_path(mut self, path: String) -> Self {
        self.snapshot_path = Some(path);
        self
    }
    
    /// Set metadata
    pub fn with_metadata(mut self, metadata: EventMetadata) -> Self {
        self.metadata = metadata;
        self
    }
    
    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
    
    /// Convert to pretty JSON string
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Detection information for event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionInfo {
    /// Detection class
    #[serde(rename = "class")]
    pub class_name: String,
    
    /// Confidence score
    pub confidence: f32,
    
    /// Bounding box
    pub bbox: BboxInfo,
}

impl From<Detection> for DetectionInfo {
    fn from(det: Detection) -> Self {
        Self {
            class_name: det.class.name().to_string(),
            confidence: det.confidence,
            bbox: BboxInfo::from(det.bbox),
        }
    }
}

/// Bounding box for JSON serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BboxInfo {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl From<BoundingBox> for BboxInfo {
    fn from(bbox: BoundingBox) -> Self {
        Self {
            x: bbox.x,
            y: bbox.y,
            width: bbox.width,
            height: bbox.height,
        }
    }
}

/// Event metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Input FPS
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps_in: Option<f32>,
    
    /// Inference FPS
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps_infer: Option<f32>,
    
    /// Inference time in ms
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_ms: Option<f32>,
    
    /// Error message (for stream events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    
    /// Frame width
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_width: Option<u32>,
    
    /// Frame height
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_height: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::DetectionClass;

    #[test]
    fn test_fire_event_creation() {
        let det = Detection::new(
            DetectionClass::Fire,
            0.9,
            BoundingBox::new(0.1, 0.1, 0.2, 0.2),
        );
        
        let event = FireEvent::fire("cam-01", "site-a", 0.9, vec![det]);
        
        assert_eq!(event.event_type, EventType::Fire);
        assert_eq!(event.camera_id, "cam-01");
        assert_eq!(event.site_id, "site-a");
        assert!((event.confidence - 0.9).abs() < 0.01);
        assert_eq!(event.detections.len(), 1);
    }

    #[test]
    fn test_event_json_serialization() {
        let event = FireEvent::fire("cam-01", "site-a", 0.85, vec![]);
        
        let json = event.to_json().unwrap();
        
        assert!(json.contains("\"event_type\":\"fire\""));
        assert!(json.contains("\"camera_id\":\"cam-01\""));
    }

    #[test]
    fn test_stream_down_event() {
        let event = FireEvent::stream_down("cam-01", "site-a", "Connection refused");
        
        assert_eq!(event.event_type, EventType::StreamDown);
        assert_eq!(
            event.metadata.error_message,
            Some("Connection refused".to_string())
        );
    }
}
