//! Configuration validation
//!
//! Validates loaded configuration for correctness and consistency.

use anyhow::{bail, Result};
use tracing::warn;

use super::loader::AppConfig;
use super::models::CameraConfig;

/// Validate the entire application configuration
pub fn validate_config(config: &AppConfig) -> Result<()> {
    validate_cameras(&config.cameras)?;
    validate_inference(&config.global.inference)?;
    validate_telegram(&config.global.telegram)?;
    
    Ok(())
}

/// Validate camera configurations
fn validate_cameras(cameras: &[CameraConfig]) -> Result<()> {
    if cameras.is_empty() {
        bail!("No cameras configured");
    }
    
    // Check for duplicate camera IDs
    let mut seen_ids = std::collections::HashSet::new();
    for camera in cameras {
        if !seen_ids.insert(&camera.camera_id) {
            bail!("Duplicate camera_id: {}", camera.camera_id);
        }
        
        validate_camera(camera)?;
    }
    
    // Warn if many cameras are disabled
    let enabled_count = cameras.iter().filter(|c| c.enabled).count();
    if enabled_count == 0 {
        warn!("All cameras are disabled!");
    } else if enabled_count < cameras.len() / 2 {
        warn!(
            enabled = enabled_count,
            total = cameras.len(),
            "More than half of cameras are disabled"
        );
    }
    
    Ok(())
}

/// Validate a single camera configuration
fn validate_camera(camera: &CameraConfig) -> Result<()> {
    // Validate camera_id format
    if camera.camera_id.is_empty() {
        bail!("Camera ID cannot be empty");
    }
    
    if camera.camera_id.len() > 50 {
        bail!(
            "Camera ID '{}' is too long (max 50 chars)",
            camera.camera_id
        );
    }
    
    // Validate RTSP URL
    if camera.enabled && camera.rtsp_url.is_empty() {
        bail!(
            "Camera '{}' has no RTSP URL configured",
            camera.camera_id
        );
    }
    
    // Validate RTSP URL format (basic check)
    if camera.enabled && !camera.rtsp_url.starts_with("rtsp://") 
        && !camera.rtsp_url.starts_with("${") 
    {
        warn!(
            camera_id = %camera.camera_id,
            url = %camera.rtsp_url,
            "RTSP URL doesn't start with rtsp://, might be invalid"
        );
    }
    
    // Validate FPS sample rate
    if camera.fps_sample == 0 {
        bail!(
            "Camera '{}' fps_sample must be > 0",
            camera.camera_id
        );
    }
    
    if camera.fps_sample > 30 {
        warn!(
            camera_id = %camera.camera_id,
            fps = camera.fps_sample,
            "High fps_sample value may cause performance issues"
        );
    }
    
    // Validate image size
    if camera.imgsz < 320 || camera.imgsz > 1280 {
        warn!(
            camera_id = %camera.camera_id,
            imgsz = camera.imgsz,
            "Unusual imgsz value, recommended: 320-1280"
        );
    }
    
    // Validate confidence thresholds
    if camera.conf_fire <= 0.0 || camera.conf_fire > 1.0 {
        bail!(
            "Camera '{}' conf_fire must be between 0 and 1",
            camera.camera_id
        );
    }
    
    if camera.conf_smoke <= 0.0 || camera.conf_smoke > 1.0 {
        bail!(
            "Camera '{}' conf_smoke must be between 0 and 1",
            camera.camera_id
        );
    }

    if camera.conf_other <= 0.0 || camera.conf_other > 1.0 {
        bail!(
            "Camera '{}' conf_other must be between 0 and 1",
            camera.camera_id
        );
    }
    
    // Validate sliding window
    if camera.window_size == 0 {
        bail!(
            "Camera '{}' window_size must be > 0",
            camera.camera_id
        );
    }
    
    if camera.fire_hits > camera.window_size {
        bail!(
            "Camera '{}' fire_hits ({}) cannot exceed window_size ({})",
            camera.camera_id,
            camera.fire_hits,
            camera.window_size
        );
    }
    
    if camera.smoke_hits > camera.window_size {
        bail!(
            "Camera '{}' smoke_hits ({}) cannot exceed window_size ({})",
            camera.camera_id,
            camera.smoke_hits,
            camera.window_size
        );
    }
    
    // Validate cooldown
    if camera.cooldown_sec < 10 {
        warn!(
            camera_id = %camera.camera_id,
            cooldown = camera.cooldown_sec,
            "Low cooldown value may cause alert spam"
        );
    }
    
    Ok(())
}

/// Validate inference configuration
fn validate_inference(config: &super::models::InferenceConfig) -> Result<()> {
    // Validate model path
    if config.model_path.is_empty() {
        bail!("Model path cannot be empty");
    }
    
    // Check if model file exists (warning only, might be in Docker)
    let model_path = std::path::Path::new(&config.model_path);
    if !model_path.exists() {
        warn!(
            path = %config.model_path,
            "Model file not found at specified path"
        );
    }
    
    // Validate device
    let valid_devices = ["cpu", "cuda", "cuda:0", "cuda:1", "tensorrt"];
    let device_lower = config.device.to_lowercase();
    if !valid_devices.iter().any(|d| device_lower.starts_with(d)) {
        warn!(
            device = %config.device,
            "Unknown device type, expected: cpu, cuda, cuda:N, or tensorrt"
        );
    }
    
    // Validate batch size
    if config.batch_size == 0 {
        bail!("Batch size must be > 0");
    }
    
    if config.batch_size > 16 {
        warn!(
            batch_size = config.batch_size,
            "Large batch size may cause OOM on RTX3060"
        );
    }
    
    Ok(())
}

/// Validate Telegram configuration
fn validate_telegram(config: &super::models::TelegramConfig) -> Result<()> {
    if !config.enabled {
        return Ok(());
    }
    
    // Validate bot token format
    if config.bot_token.is_empty() || config.bot_token.starts_with("$") {
        // Might be env var placeholder at runtime
        warn!("Telegram bot_token appears to be unset or using env var");
    } else if !config.bot_token.contains(':') {
        warn!("Telegram bot_token format appears invalid (expected format: 123456:ABC-DEF...)");
    }
    
    // Validate chat ID
    if config.default_chat_id.is_empty() {
        warn!("Telegram default_chat_id is not configured");
    }
    
    // Validate rate limits
    if config.rate_limit.max_per_minute == 0 {
        warn!("Telegram rate limit is 0, no messages will be sent");
    }
    
    if config.rate_limit.cooldown_sec < 10 {
        warn!(
            cooldown = config.rate_limit.cooldown_sec,
            "Low Telegram cooldown may cause rate limiting by Telegram API"
        );
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::models::*;

    fn create_valid_camera() -> CameraConfig {
        CameraConfig {
            camera_id: "cam-01".to_string(),
            site_id: "site-a".to_string(),
            name: "Test Camera".to_string(),
            description: None,
            rtsp_url: "rtsp://localhost/stream".to_string(),
            enabled: true,
            fps_sample: 3,
            imgsz: 640,
            conf_fire: 0.5,
            conf_smoke: 0.4,
            conf_other: 0.4,
            window_size: 10,
            fire_hits: 3,
            smoke_hits: 4,
            cooldown_sec: 60,
        }
    }

    #[test]
    fn test_validate_camera_valid() {
        let camera = create_valid_camera();
        assert!(validate_camera(&camera).is_ok());
    }

    #[test]
    fn test_validate_camera_empty_id() {
        let mut camera = create_valid_camera();
        camera.camera_id = "".to_string();
        assert!(validate_camera(&camera).is_err());
    }

    #[test]
    fn test_validate_camera_invalid_confidence() {
        let mut camera = create_valid_camera();
        camera.conf_fire = 1.5;
        assert!(validate_camera(&camera).is_err());
    }

    #[test]
    fn test_validate_camera_hits_exceed_window() {
        let mut camera = create_valid_camera();
        camera.fire_hits = 15;
        camera.window_size = 10;
        assert!(validate_camera(&camera).is_err());
    }

    #[test]
    fn test_validate_cameras_duplicate_id() {
        let camera1 = create_valid_camera();
        let camera2 = create_valid_camera();
        let cameras = vec![camera1, camera2];
        assert!(validate_cameras(&cameras).is_err());
    }
}
