//! Error types for the detector service
//!
//! Sử dụng thiserror để định nghĩa các error types với proper error handling.

use thiserror::Error;

/// Main error type for detector service
#[derive(Error, Debug)]
pub enum DetectorError {
    // -------------------------------------------------------------------------
    // Configuration Errors
    // -------------------------------------------------------------------------
    #[error("Configuration error: {0}")]
    ConfigError(#[from] config::ConfigError),

    #[error("Invalid configuration: {message}")]
    InvalidConfig { message: String },

    #[error("Camera configuration invalid for '{camera_id}': {message}")]
    InvalidCameraConfig { camera_id: String, message: String },

    // -------------------------------------------------------------------------
    // Camera/Stream Errors
    // -------------------------------------------------------------------------
    #[error("Failed to connect to camera '{camera_id}': {message}")]
    CameraConnectionFailed { camera_id: String, message: String },

    #[error("Stream error for camera '{camera_id}': {message}")]
    StreamError { camera_id: String, message: String },

    #[error("GStreamer error: {0}")]
    GStreamerError(String),

    #[error("Camera '{camera_id}' not found")]
    CameraNotFound { camera_id: String },

    // -------------------------------------------------------------------------
    // Inference Errors
    // -------------------------------------------------------------------------
    #[error("Failed to load ONNX model: {0}")]
    ModelLoadError(String),

    #[error("Inference error: {0}")]
    InferenceError(String),

    #[error("Invalid input shape: expected {expected}, got {actual}")]
    InvalidInputShape { expected: String, actual: String },

    #[error("CUDA error: {0}")]
    CudaError(String),

    // -------------------------------------------------------------------------
    // Event Errors
    // -------------------------------------------------------------------------
    #[error("Failed to publish event: {0}")]
    EventPublishError(String),

    #[error("Snapshot encoding failed: {0}")]
    SnapshotError(String),

    // -------------------------------------------------------------------------
    // IO Errors
    // -------------------------------------------------------------------------
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("File not found: {path}")]
    FileNotFound { path: String },

    // -------------------------------------------------------------------------
    // Generic Errors
    // -------------------------------------------------------------------------
    #[error("Internal error: {0}")]
    Internal(String),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

/// Result type alias for detector operations
pub type DetectorResult<T> = Result<T, DetectorError>;

// -------------------------------------------------------------------------
// Error Conversion Implementations
// -------------------------------------------------------------------------

impl From<gstreamer::glib::Error> for DetectorError {
    fn from(err: gstreamer::glib::Error) -> Self {
        DetectorError::GStreamerError(err.to_string())
    }
}

impl From<ort::Error> for DetectorError {
    fn from(err: ort::Error) -> Self {
        DetectorError::InferenceError(err.to_string())
    }
}

impl From<image::ImageError> for DetectorError {
    fn from(err: image::ImageError) -> Self {
        DetectorError::SnapshotError(err.to_string())
    }
}

// -------------------------------------------------------------------------
// Error Helpers
// -------------------------------------------------------------------------

impl DetectorError {
    /// Check if error is recoverable (should retry)
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            DetectorError::CameraConnectionFailed { .. }
                | DetectorError::StreamError { .. }
                | DetectorError::EventPublishError(_)
        )
    }

    /// Check if error is related to configuration
    pub fn is_config_error(&self) -> bool {
        matches!(
            self,
            DetectorError::ConfigError(_)
                | DetectorError::InvalidConfig { .. }
                | DetectorError::InvalidCameraConfig { .. }
        )
    }

    /// Get the camera_id if this error is camera-specific
    pub fn camera_id(&self) -> Option<&str> {
        match self {
            DetectorError::CameraConnectionFailed { camera_id, .. } => Some(camera_id),
            DetectorError::StreamError { camera_id, .. } => Some(camera_id),
            DetectorError::InvalidCameraConfig { camera_id, .. } => Some(camera_id),
            DetectorError::CameraNotFound { camera_id } => Some(camera_id),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_is_recoverable() {
        let err = DetectorError::CameraConnectionFailed {
            camera_id: "cam-01".to_string(),
            message: "Connection refused".to_string(),
        };
        assert!(err.is_recoverable());

        let err = DetectorError::InvalidConfig {
            message: "Missing field".to_string(),
        };
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_error_camera_id() {
        let err = DetectorError::StreamError {
            camera_id: "cam-01".to_string(),
            message: "EOF".to_string(),
        };
        assert_eq!(err.camera_id(), Some("cam-01"));

        let err = DetectorError::ModelLoadError("Not found".to_string());
        assert_eq!(err.camera_id(), None);
    }
}
