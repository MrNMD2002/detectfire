//! Configuration loader
//!
//! Loads and merges camera and settings configurations from YAML files.

use std::path::PathBuf;
use std::env;
use anyhow::{Context, Result};
use tracing::{info, debug, warn};

use super::models::*;
use super::validation::validate_config;

/// Application configuration combining cameras and settings
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// List of camera configurations
    pub cameras: Vec<CameraConfig>,
    
    /// Global settings
    pub global: GlobalConfig,
}

/// Global configuration from settings.yaml
#[derive(Debug, Clone)]
pub struct GlobalConfig {
    pub server: ServerConfig,
    pub inference: InferenceConfig,
    pub reconnect: ReconnectConfig,
    pub telegram: TelegramConfig,
    pub logging: LoggingConfig,
    pub storage: StorageConfig,
    pub monitoring: MonitoringConfig,
}

impl AppConfig {
    /// Load configuration from default paths or environment variables
    pub fn load() -> Result<Self> {
        let config_dir = Self::get_config_dir()?;
        
        let cameras_path = config_dir.join("cameras.yaml");
        let settings_path = config_dir.join("settings.yaml");
        
        Self::load_from_paths(&cameras_path, &settings_path)
    }
    
    /// Load configuration from specific paths
    pub fn load_from_paths(cameras_path: &PathBuf, settings_path: &PathBuf) -> Result<Self> {
        info!(
            cameras_path = %cameras_path.display(),
            settings_path = %settings_path.display(),
            "Loading configuration"
        );
        
        // Load cameras configuration
        let cameras_config = Self::load_cameras_config(cameras_path)?;
        
        // Load settings configuration
        let settings_config = Self::load_settings_config(settings_path)?;
        
        // Merge and apply defaults
        let cameras = Self::apply_camera_defaults(
            cameras_config.cameras,
            &cameras_config.global,
        );
        
        // Build global config
        // Prioritize inference config from settings.yaml; fallback to cameras.yaml global defaults
        let inference = if settings_config.inference.model_path != default_model_path()
            || settings_config.inference.device != default_device() {
            // settings.yaml has explicit inference config — use it
            settings_config.inference
        } else {
            // Fallback to cameras.yaml global defaults
            cameras_config.global.inference
        };
        
        let global = GlobalConfig {
            server: settings_config.server,
            inference,
            reconnect: cameras_config.global.reconnect,
            telegram: settings_config.telegram,
            logging: settings_config.logging,
            storage: settings_config.storage,
            monitoring: settings_config.monitoring,
        };
        
        let config = Self { cameras, global };
        
        // Validate configuration
        validate_config(&config)?;
        
        info!(
            enabled_cameras = config.cameras.iter().filter(|c| c.enabled).count(),
            total_cameras = config.cameras.len(),
            "Configuration loaded successfully"
        );
        
        Ok(config)
    }
    
    /// Get the configuration directory
    fn get_config_dir() -> Result<PathBuf> {
        // Check environment variable first
        if let Ok(path) = env::var("CONFIG_DIR") {
            return Ok(PathBuf::from(path));
        }
        
        // Check common locations
        let candidates = vec![
            PathBuf::from("./configs"),
            PathBuf::from("/etc/fire-detect"),
            PathBuf::from("/app/configs"),
        ];
        
        for path in candidates {
            if path.exists() {
                return Ok(path);
            }
        }
        
        // Default to current directory configs
        Ok(PathBuf::from("./configs"))
    }
    
    /// Load cameras configuration from YAML
    fn load_cameras_config(path: &PathBuf) -> Result<CamerasConfig> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read cameras config: {}", path.display()))?;
        
        // Expand environment variables in content
        let content = Self::expand_env_vars(&content);
        
        let config: CamerasConfig = serde_yaml::from_str(&content)
            .with_context(|| "Failed to parse cameras.yaml")?;
        
        debug!(cameras = config.cameras.len(), "Parsed cameras configuration");
        
        Ok(config)
    }
    
    /// Load settings configuration from YAML
    fn load_settings_config(path: &PathBuf) -> Result<SettingsConfig> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read settings config: {}", path.display()))?;
        
        // Expand environment variables in content
        let content = Self::expand_env_vars(&content);
        
        let config: SettingsConfig = serde_yaml::from_str(&content)
            .with_context(|| "Failed to parse settings.yaml")?;
        
        debug!("Parsed settings configuration");
        
        Ok(config)
    }
    
    /// Apply global defaults to camera configurations
    fn apply_camera_defaults(
        cameras: Vec<CameraConfig>,
        defaults: &GlobalDefaults,
    ) -> Vec<CameraConfig> {
        cameras
            .into_iter()
            .map(|mut cam| {
                // Only apply defaults if camera value equals the struct default
                // This preserves explicitly set values
                if cam.fps_sample == 0 {
                    cam.fps_sample = defaults.default_fps_sample;
                }
                if cam.imgsz == 0 {
                    cam.imgsz = defaults.default_imgsz;
                }
                // For floats, check if they're at the default (could be explicitly set)
                // We'll trust the serde defaults here
                
                cam
            })
            .collect()
    }
    
    /// Expand environment variables in configuration content
    /// 
    /// Supports:
    /// - `${VAR}` or `${VAR:-default}`
    /// - `$VAR`
    fn expand_env_vars(content: &str) -> String {
        let mut result = content.to_string();
        
        // Pattern: ${VAR:-default} or ${VAR}
        let re = regex::Regex::new(r"\$\{([A-Z_][A-Z0-9_]*)(?::-([^}]*))?\}").unwrap();
        
        result = re.replace_all(&result, |caps: &regex::Captures| {
            let var_name = &caps[1];
            let default_value = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            
            match env::var(var_name) {
                Ok(value) => value,
                Err(_) => {
                    if !default_value.is_empty() {
                        default_value.to_string()
                    } else {
                        warn!(var = var_name, "Environment variable not set, using empty string");
                        String::new()
                    }
                }
            }
        }).to_string();
        
        // Pattern: $VAR (without braces)
        let re_simple = regex::Regex::new(r"\$([A-Z_][A-Z0-9_]*)").unwrap();
        
        result = re_simple.replace_all(&result, |caps: &regex::Captures| {
            let var_name = &caps[1];
            env::var(var_name).unwrap_or_else(|_| {
                warn!(var = var_name, "Environment variable not set, using empty string");
                String::new()
            })
        }).to_string();
        
        result
    }
    
    /// Get camera configuration by ID
    pub fn get_camera(&self, camera_id: &str) -> Option<&CameraConfig> {
        self.cameras.iter().find(|c| c.camera_id == camera_id)
    }
    
    /// Get all enabled cameras
    pub fn enabled_cameras(&self) -> Vec<&CameraConfig> {
        self.cameras.iter().filter(|c| c.enabled).collect()
    }
    
    /// Get cameras grouped by site
    pub fn cameras_by_site(&self) -> std::collections::HashMap<String, Vec<&CameraConfig>> {
        let mut map = std::collections::HashMap::new();
        
        for camera in &self.cameras {
            map.entry(camera.site_id.clone())
                .or_insert_with(Vec::new)
                .push(camera);
        }
        
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_env_vars() {
        env::set_var("TEST_VAR", "test_value");
        
        let input = "url: ${TEST_VAR}";
        let result = AppConfig::expand_env_vars(input);
        assert_eq!(result, "url: test_value");
        
        let input_default = "url: ${MISSING_VAR:-default_value}";
        let result = AppConfig::expand_env_vars(input_default);
        assert_eq!(result, "url: default_value");
        
        env::remove_var("TEST_VAR");
    }
}
