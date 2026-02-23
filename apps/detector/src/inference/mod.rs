//! Inference module for YOLOv10 fire/smoke detection
//!
//! This module handles:
//! - ONNX Runtime session management
//! - Image preprocessing (resize, normalize)
//! - Model inference on GPU
//! - Post-processing (NMS, decode detections)

mod engine;
mod preprocess;
mod postprocess;
mod detection;

pub use engine::InferenceEngine;
pub use detection::{Detection, DetectionClass, InferenceResult, BoundingBox};
