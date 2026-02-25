//! Configuration data models
//!
//! Định nghĩa tất cả các struct cho camera và global configuration.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Camera Configuration
// =============================================================================

/// Configuration for a single camera
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CameraConfig {
    /// Unique identifier for the camera
    pub camera_id: String,
    
    /// Site/location identifier
    pub site_id: String,
    
    /// Human-readable name
    pub name: String,
    
    /// Optional description
    #[serde(default)]
    pub description: Option<String>,
    
    /// RTSP URL (supports env var substitution)
    pub rtsp_url: String,
    
    /// Whether camera is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    
    /// Frame sampling rate (frames per second)
    #[serde(default = "default_fps_sample")]
    pub fps_sample: u32,
    
    /// Input image size for inference
    #[serde(default = "default_imgsz")]
    pub imgsz: u32,
    
    /// Confidence threshold for fire detection
    #[serde(default = "default_conf_fire")]
    pub conf_fire: f32,
    
    /// Confidence threshold for smoke detection
    #[serde(default = "default_conf_smoke")]
    pub conf_smoke: f32,

    /// Confidence threshold for "other" (fire-related indicators), YOLOv26 class 2
    #[serde(default = "default_conf_other")]
    pub conf_other: f32,
    
    /// Sliding window size for decision engine
    #[serde(default = "default_window_size")]
    pub window_size: usize,
    
    /// Minimum fire detections in window to trigger alert
    #[serde(default = "default_fire_hits")]
    pub fire_hits: usize,
    
    /// Minimum smoke detections in window to trigger alert
    #[serde(default = "default_smoke_hits")]
    pub smoke_hits: usize,
    
    /// Cooldown between alerts (seconds)
    #[serde(default = "default_cooldown_sec")]
    pub cooldown_sec: u64,

    /// Video codec: "h264" or "h265" (default: "h264")
    #[serde(default = "default_codec")]
    pub codec: String,
}

// Default value functions
fn default_enabled() -> bool { true }
fn default_codec() -> String { "h264".to_string() }
fn default_fps_sample() -> u32 { 3 }
fn default_imgsz() -> u32 { 640 }
fn default_conf_fire() -> f32 { 0.5 }
fn default_conf_smoke() -> f32 { 0.4 }
fn default_conf_other() -> f32 { 0.4 }
fn default_window_size() -> usize { 10 }
fn default_fire_hits() -> usize { 3 }
fn default_smoke_hits() -> usize { 4 }
fn default_cooldown_sec() -> u64 { 60 }

// =============================================================================
// Global Configuration
// =============================================================================

/// Root configuration for cameras.yaml
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CamerasConfig {
    /// Global defaults (supports both "global" and "defaults" keys in YAML)
    #[serde(default, alias = "defaults")]
    pub global: GlobalDefaults,
    
    /// List of camera configurations
    pub cameras: Vec<CameraConfig>,
}

/// Global default values
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GlobalDefaults {
    #[serde(default = "default_fps_sample")]
    pub default_fps_sample: u32,
    
    #[serde(default = "default_imgsz")]
    pub default_imgsz: u32,
    
    #[serde(default = "default_conf_fire")]
    pub default_conf_fire: f32,
    
    #[serde(default = "default_conf_smoke")]
    pub default_conf_smoke: f32,
    
    #[serde(default = "default_window_size")]
    pub default_window_size: usize,
    
    #[serde(default = "default_fire_hits")]
    pub default_fire_hits: usize,
    
    #[serde(default = "default_smoke_hits")]
    pub default_smoke_hits: usize,
    
    #[serde(default = "default_cooldown_sec")]
    pub default_cooldown_sec: u64,
    
    /// Reconnection settings
    #[serde(default)]
    pub reconnect: ReconnectConfig,
    
    /// Inference settings
    #[serde(default)]
    pub inference: InferenceConfig,
}

/// Reconnection configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReconnectConfig {
    /// Initial delay before first reconnect attempt (ms)
    #[serde(default = "default_initial_delay")]
    pub initial_delay_ms: u64,
    
    /// Maximum delay between reconnect attempts (ms)
    #[serde(default = "default_max_delay")]
    pub max_delay_ms: u64,
    
    /// Maximum number of reconnect attempts
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    
    /// Backoff multiplier
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
}

fn default_initial_delay() -> u64 { 1000 }
fn default_max_delay() -> u64 { 30000 }
fn default_max_retries() -> u32 { 10 }
fn default_backoff_multiplier() -> f64 { 2.0 }

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            initial_delay_ms: default_initial_delay(),
            max_delay_ms: default_max_delay(),
            max_retries: default_max_retries(),
            backoff_multiplier: default_backoff_multiplier(),
        }
    }
}

/// Inference engine configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InferenceConfig {
    /// Batch size for inference (if supported)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    
    /// Number of warmup frames
    #[serde(default = "default_warmup_frames")]
    pub warmup_frames: usize,
    
    /// Device to use (cuda:0, cpu, etc.)
    #[serde(default = "default_device")]
    pub device: String,
    
    /// Path to ONNX model
    #[serde(default = "default_model_path")]
    pub model_path: String,
    
    /// Number of inference threads
    #[serde(default = "default_num_threads")]
    pub num_threads: usize,
}

fn default_batch_size() -> usize { 1 }
fn default_warmup_frames() -> usize { 5 }
pub(crate) fn default_device() -> String { "cuda:0".to_string() }
pub(crate) fn default_model_path() -> String { "models/best.onnx".to_string() }
fn default_num_threads() -> usize { 4 }

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            batch_size: default_batch_size(),
            warmup_frames: default_warmup_frames(),
            device: default_device(),
            model_path: default_model_path(),
            num_threads: default_num_threads(),
        }
    }
}

// =============================================================================
// Settings Configuration
// =============================================================================

/// Root configuration for settings.yaml
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SettingsConfig {
    /// Server configuration
    pub server: ServerConfig,
    
    /// Database configuration
    pub database: DatabaseConfig,
    
    /// Authentication configuration
    pub auth: AuthConfig,
    
    /// Telegram configuration
    pub telegram: TelegramConfig,
    
    /// Logging configuration
    #[serde(default)]
    pub logging: LoggingConfig,
    
    /// Storage configuration
    #[serde(default)]
    pub storage: StorageConfig,
    
    /// Monitoring configuration
    #[serde(default)]
    pub monitoring: MonitoringConfig,
    
    /// Inference configuration (from settings.yaml)
    #[serde(default)]
    pub inference: InferenceConfig,
}

/// Server configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub api: ApiServerConfig,
    pub detector: DetectorServerConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_api_port")]
    pub port: u16,
    #[serde(default = "default_workers")]
    pub workers: usize,
}

fn default_host() -> String { "0.0.0.0".to_string() }
fn default_api_port() -> u16 { 8080 }
fn default_workers() -> usize { 4 }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DetectorServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_grpc_port")]
    pub grpc_port: u16,
}

fn default_grpc_port() -> u16 { 50051 }

/// Database configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub host: String,
    #[serde(default = "default_db_port")]
    pub port: u16,
    pub name: String,
    pub user: String,
    pub password: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    #[serde(default)]
    pub encryption_key: Option<String>,
}

fn default_db_port() -> u16 { 5432 }
fn default_max_connections() -> u32 { 20 }
fn default_min_connections() -> u32 { 5 }

/// Authentication configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub jwt_secret: String,
    #[serde(default = "default_jwt_expiry")]
    pub jwt_expiry_hours: u64,
    #[serde(default = "default_bcrypt_cost")]
    pub bcrypt_cost: u32,
}

fn default_jwt_expiry() -> u64 { 24 }
fn default_bcrypt_cost() -> u32 { 12 }

/// Telegram configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    pub bot_token: String,
    pub default_chat_id: String,
    #[serde(default)]
    pub rate_limit: TelegramRateLimit,
    #[serde(default)]
    pub templates: TelegramTemplates,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TelegramRateLimit {
    #[serde(default = "default_max_per_minute")]
    pub max_per_minute: u32,
    #[serde(default = "default_cooldown_sec")]
    pub cooldown_sec: u64,
}

fn default_max_per_minute() -> u32 { 10 }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TelegramTemplates {
    pub fire: String,
    pub smoke: String,
    pub stream_down: String,
    pub stream_up: String,
}

impl Default for TelegramTemplates {
    fn default() -> Self {
        Self {
            fire: "🔥 *CẢNH BÁO CHÁY*\n📍 Camera: {camera_name}\n🏢 Site: {site_id}\n⏰ Thời gian: {timestamp}\n📊 Độ tin cậy: {confidence}%".to_string(),
            smoke: "💨 *PHÁT HIỆN KHÓI*\n📍 Camera: {camera_name}\n🏢 Site: {site_id}\n⏰ Thời gian: {timestamp}\n📊 Độ tin cậy: {confidence}%".to_string(),
            stream_down: "⚠️ *MẤT KẾT NỐI CAMERA*\n📍 Camera: {camera_name}\n🏢 Site: {site_id}\n⏰ Thời gian: {timestamp}".to_string(),
            stream_up: "✅ *ĐÃ KẾT NỐI LẠI*\n📍 Camera: {camera_name}\n🏢 Site: {site_id}\n⏰ Thời gian: {timestamp}".to_string(),
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
}

fn default_log_level() -> String { "info".to_string() }
fn default_log_format() -> String { "json".to_string() }

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
        }
    }
}

/// Storage configuration
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct StorageConfig {
    #[serde(default)]
    pub snapshots: SnapshotConfig,
    #[serde(default)]
    pub minio: MinioConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SnapshotConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_snapshot_path")]
    pub path: String,
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    #[serde(default = "default_max_size_mb")]
    pub max_size_mb: u64,
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_quality")]
    pub quality: u8,
}

fn default_true() -> bool { true }
fn default_snapshot_path() -> String { "/data/snapshots".to_string() }
fn default_retention_days() -> u32 { 30 }
fn default_max_size_mb() -> u64 { 10240 }
fn default_format() -> String { "jpeg".to_string() }
fn default_quality() -> u8 { 85 }

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: default_snapshot_path(),
            retention_days: default_retention_days(),
            max_size_mb: default_max_size_mb(),
            format: default_format(),
            quality: default_quality(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MinioConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub access_key: String,
    #[serde(default)]
    pub secret_key: String,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub secure: bool,
}

/// Monitoring configuration
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MonitoringConfig {
    #[serde(default)]
    pub health: HealthConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub thresholds: ThresholdConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_health_path")]
    pub path: String,
}

fn default_health_path() -> String { "/health".to_string() }

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: default_health_path(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MetricsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_metrics_path")]
    pub path: String,
    #[serde(default = "default_metrics_port")]
    pub port: u16,
}

fn default_metrics_path() -> String { "/metrics".to_string() }
fn default_metrics_port() -> u16 { 9090 }

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: default_metrics_path(),
            port: default_metrics_port(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ThresholdConfig {
    #[serde(default = "default_max_inference_ms")]
    pub max_inference_ms: u64,
    #[serde(default = "default_max_queue_size")]
    pub max_queue_size: usize,
    #[serde(default = "default_min_fps")]
    pub min_fps: f32,
}

fn default_max_inference_ms() -> u64 { 100 }
fn default_max_queue_size() -> usize { 10 }
fn default_min_fps() -> f32 { 1.0 }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_config_defaults() {
        let yaml = r#"
            camera_id: cam-01
            site_id: site-a
            name: Test Camera
            rtsp_url: rtsp://localhost/stream
        "#;
        
        let config: CameraConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.fps_sample, 3);
        assert_eq!(config.imgsz, 640);
        assert!((config.conf_fire - 0.5).abs() < 0.001);
    }
}
