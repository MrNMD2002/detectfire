//! Detection result types
//!
//! Defines the output types for inference results.

use serde::{Deserialize, Serialize};

/// Detection class (YOLOv26 SalahALHaismawi/yolov26-fire-detection)
/// Model class mapping: {0: 'fire', 1: 'other', 2: 'smoke'}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionClass {
    Fire,
    Smoke,
    /// Related fire indicators (YOLOv26 class 1)
    Other,
}

impl DetectionClass {
    /// Create from class index (0=fire, 1=other, 2=smoke)
    /// Matches model.names: {0: 'fire', 1: 'other', 2: 'smoke'}
    pub fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Fire),
            1 => Some(Self::Other),
            2 => Some(Self::Smoke),
            _ => None,
        }
    }

    /// Get class index (matching model output)
    pub fn index(&self) -> usize {
        match self {
            Self::Fire => 0,
            Self::Other => 1,
            Self::Smoke => 2,
        }
    }

    /// Get class name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Fire => "fire",
            Self::Smoke => "smoke",
            Self::Other => "other",
        }
    }
}

impl std::fmt::Display for DetectionClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Bounding box in normalized coordinates [0, 1]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BoundingBox {
    /// Top-left x coordinate
    pub x: f32,
    /// Top-left y coordinate
    pub y: f32,
    /// Width
    pub width: f32,
    /// Height
    pub height: f32,
}

impl BoundingBox {
    /// Create a new bounding box
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
    
    /// Create from center coordinates (YOLO format)
    pub fn from_center(cx: f32, cy: f32, w: f32, h: f32) -> Self {
        Self {
            x: cx - w / 2.0,
            y: cy - h / 2.0,
            width: w,
            height: h,
        }
    }
    
    /// Get center coordinates
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
    
    /// Get area
    pub fn area(&self) -> f32 {
        self.width * self.height
    }
    
    /// Calculate IoU (Intersection over Union) with another box
    pub fn iou(&self, other: &Self) -> f32 {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.width).min(other.x + other.width);
        let y2 = (self.y + self.height).min(other.y + other.height);
        
        let intersection = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
        let union = self.area() + other.area() - intersection;
        
        if union > 0.0 {
            intersection / union
        } else {
            0.0
        }
    }
    
    /// Scale to pixel coordinates
    pub fn to_pixels(&self, width: u32, height: u32) -> (i32, i32, i32, i32) {
        let x = (self.x * width as f32) as i32;
        let y = (self.y * height as f32) as i32;
        let w = (self.width * width as f32) as i32;
        let h = (self.height * height as f32) as i32;
        (x, y, w, h)
    }
}

/// A single detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    /// Detection class
    pub class: DetectionClass,
    
    /// Confidence score [0, 1]
    pub confidence: f32,
    
    /// Bounding box (normalized coordinates)
    pub bbox: BoundingBox,
}

impl Detection {
    /// Create a new detection
    pub fn new(class: DetectionClass, confidence: f32, bbox: BoundingBox) -> Self {
        Self { class, confidence, bbox }
    }
    
    /// Check if detection is above threshold
    pub fn is_above_threshold(&self, fire_thresh: f32, smoke_thresh: f32, other_thresh: f32) -> bool {
        match self.class {
            DetectionClass::Fire => self.confidence >= fire_thresh,
            DetectionClass::Smoke => self.confidence >= smoke_thresh,
            DetectionClass::Other => self.confidence >= other_thresh,
        }
    }
}

/// Result of inference on a single frame
#[derive(Debug, Clone, Default)]
pub struct InferenceResult {
    /// All detections
    pub detections: Vec<Detection>,
    
    /// Inference time in milliseconds
    pub inference_ms: f32,
    
    /// Preprocessing time in milliseconds
    pub preprocess_ms: f32,
    
    /// Postprocessing time in milliseconds
    pub postprocess_ms: f32,
}

impl InferenceResult {
    /// Create empty result
    pub fn empty() -> Self {
        Self::default()
    }
    
    /// Get fire detections
    pub fn fire_detections(&self) -> Vec<&Detection> {
        self.detections
            .iter()
            .filter(|d| d.class == DetectionClass::Fire)
            .collect()
    }
    
    /// Get smoke detections
    pub fn smoke_detections(&self) -> Vec<&Detection> {
        self.detections
            .iter()
            .filter(|d| d.class == DetectionClass::Smoke)
            .collect()
    }

    /// Get other (fire-related) detections
    pub fn other_detections(&self) -> Vec<&Detection> {
        self.detections
            .iter()
            .filter(|d| d.class == DetectionClass::Other)
            .collect()
    }

    /// Get highest confidence fire detection
    pub fn best_fire(&self) -> Option<&Detection> {
        self.fire_detections()
            .into_iter()
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap())
    }
    
    /// Get highest confidence smoke detection
    pub fn best_smoke(&self) -> Option<&Detection> {
        self.smoke_detections()
            .into_iter()
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap())
    }
    
    /// Check if any fire detected
    pub fn has_fire(&self) -> bool {
        self.detections.iter().any(|d| d.class == DetectionClass::Fire)
    }
    
    /// Check if any smoke detected
    pub fn has_smoke(&self) -> bool {
        self.detections.iter().any(|d| d.class == DetectionClass::Smoke)
    }

    /// Check if any other (fire-related) detected
    pub fn has_other(&self) -> bool {
        self.detections.iter().any(|d| d.class == DetectionClass::Other)
    }

    /// Total processing time
    pub fn total_ms(&self) -> f32 {
        self.preprocess_ms + self.inference_ms + self.postprocess_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detection_class_from_index() {
        // model.names: {0: 'fire', 1: 'other', 2: 'smoke'}
        assert_eq!(DetectionClass::from_index(0), Some(DetectionClass::Fire));
        assert_eq!(DetectionClass::from_index(1), Some(DetectionClass::Other));
        assert_eq!(DetectionClass::from_index(2), Some(DetectionClass::Smoke));
        assert_eq!(DetectionClass::from_index(3), None);
    }

    #[test]
    fn test_bounding_box_iou() {
        let box1 = BoundingBox::new(0.0, 0.0, 0.5, 0.5);
        let box2 = BoundingBox::new(0.25, 0.25, 0.5, 0.5);
        
        let iou = box1.iou(&box2);
        assert!(iou > 0.1 && iou < 0.3, "Expected IoU ~0.14, got {}", iou);
    }

    #[test]
    fn test_bounding_box_from_center() {
        let bbox = BoundingBox::from_center(0.5, 0.5, 0.4, 0.4);
        assert!((bbox.x - 0.3).abs() < 0.001);
        assert!((bbox.y - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_inference_result_filtering() {
        let result = InferenceResult {
            detections: vec![
                Detection::new(DetectionClass::Fire, 0.9, BoundingBox::new(0.1, 0.1, 0.2, 0.2)),
                Detection::new(DetectionClass::Smoke, 0.8, BoundingBox::new(0.3, 0.3, 0.2, 0.2)),
                Detection::new(DetectionClass::Fire, 0.7, BoundingBox::new(0.5, 0.5, 0.2, 0.2)),
            ],
            ..Default::default()
        };
        
        assert_eq!(result.fire_detections().len(), 2);
        assert_eq!(result.smoke_detections().len(), 1);
        assert!(result.has_fire());
        assert!(result.has_smoke());
    }
}
