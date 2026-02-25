//! Data models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

/// Camera model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Camera {
    pub id: Uuid,
    pub site_id: String,
    pub name: String,
    pub description: Option<String>,
    /// Detector config camera_id for HLS stream mapping (e.g. "cam-01")
    pub detector_camera_id: Option<String>,
    #[serde(skip_serializing)]
    pub rtsp_url: String,
    pub enabled: bool,
    pub codec: String,
    pub fps_sample: u32,
    pub imgsz: u32,
    pub conf_fire: f32,
    pub conf_smoke: f32,
    pub conf_other: f32,
    pub window_size: u32,
    pub fire_hits: u32,
    pub smoke_hits: u32,
    pub cooldown_sec: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Camera creation input
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateCameraInput {
    #[validate(length(min = 1, max = 50))]
    pub site_id: String,
    
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    
    #[validate(length(max = 500))]
    pub description: Option<String>,
    
    /// Detector config camera_id for stream/event mapping (e.g. "cam-01")
    pub detector_camera_id: Option<String>,
    
    #[validate(url)]
    pub rtsp_url: String,
    
    pub enabled: Option<bool>,

    pub codec: Option<String>,

    #[validate(range(min = 1, max = 30))]
    pub fps_sample: Option<u32>,

    #[validate(range(min = 320, max = 1280))]
    pub imgsz: Option<u32>,

    #[validate(range(min = 0.1, max = 1.0))]
    pub conf_fire: Option<f32>,

    #[validate(range(min = 0.1, max = 1.0))]
    pub conf_smoke: Option<f32>,

    #[validate(range(min = 0.1, max = 1.0))]
    pub conf_other: Option<f32>,

    #[validate(range(min = 1, max = 100))]
    pub window_size: Option<u32>,
    
    #[validate(range(min = 1, max = 50))]
    pub fire_hits: Option<u32>,
    
    #[validate(range(min = 1, max = 50))]
    pub smoke_hits: Option<u32>,
    
    #[validate(range(min = 5, max = 3600))]
    pub cooldown_sec: Option<u64>,
}

/// Camera update input — validators mirror CreateCameraInput to prevent invalid data
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateCameraInput {
    #[validate(length(min = 1, max = 50))]
    pub site_id: Option<String>,

    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,

    #[validate(length(max = 500))]
    pub description: Option<String>,

    #[validate(url)]
    pub rtsp_url: Option<String>,

    pub detector_camera_id: Option<String>,

    pub enabled: Option<bool>,

    pub codec: Option<String>,

    #[validate(range(min = 1, max = 30))]
    pub fps_sample: Option<u32>,

    #[validate(range(min = 320, max = 1280))]
    pub imgsz: Option<u32>,

    #[validate(range(min = 0.1, max = 1.0))]
    pub conf_fire: Option<f32>,

    #[validate(range(min = 0.1, max = 1.0))]
    pub conf_smoke: Option<f32>,

    #[validate(range(min = 0.1, max = 1.0))]
    pub conf_other: Option<f32>,

    #[validate(range(min = 1, max = 100))]
    pub window_size: Option<u32>,

    #[validate(range(min = 1, max = 50))]
    pub fire_hits: Option<u32>,

    #[validate(range(min = 1, max = 50))]
    pub smoke_hits: Option<u32>,

    #[validate(range(min = 5, max = 3600))]
    pub cooldown_sec: Option<u64>,
}

/// Detection event model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: Uuid,
    pub event_type: String,
    pub camera_id: Uuid,
    /// Denormalized camera name (from LEFT JOIN cameras) — None when created internally
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_name: Option<String>,
    pub site_id: String,
    pub timestamp: DateTime<Utc>,
    pub confidence: f32,
    pub detections: serde_json::Value,
    pub snapshot_path: Option<String>,
    pub metadata: serde_json::Value,
    pub acknowledged: bool,
    pub acknowledged_by: Option<Uuid>,
    pub acknowledged_at: Option<DateTime<Utc>>,
}

/// Event creation input
#[derive(Debug, Clone, Deserialize)]
pub struct CreateEventInput {
    pub event_type: String,
    pub camera_id: Uuid,
    pub site_id: String,
    pub timestamp: DateTime<Utc>,
    pub confidence: f32,
    pub detections: serde_json::Value,
    pub snapshot_path: Option<String>,
    pub metadata: serde_json::Value,
}

/// Event filter
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EventFilter {
    pub camera_id: Option<Uuid>,
    pub site_id: Option<String>,
    pub event_type: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub acknowledged: Option<bool>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

/// User model
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub name: String,
    pub role: String,
    pub active: bool,
    pub telegram_chat_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Login request
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(email)]
    pub email: String,
    
    #[validate(length(min = 6))]
    pub password: String,
}

/// Login response
#[derive(Debug, Clone, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user: UserInfo,
}

/// User info (public)
#[derive(Debug, Clone, Serialize)]
pub struct UserInfo {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
}

/// Camera status (from detector)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraStatus {
    pub camera_id: Uuid,
    pub name: String,
    pub status: String,
    pub reconnect_count: u32,
    pub fps_in: f32,
    pub fps_infer: f32,
    pub last_frame_timestamp: i64,
    pub error_message: Option<String>,
}

/// Health response
#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: i64,
    pub database: String,
    pub detector: String,
}

/// Pagination wrapper
#[derive(Debug, Clone, Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: i32,
    pub per_page: i32,
    pub total_pages: i32,
}
