//! Application configuration

use anyhow::Result;
use serde::Deserialize;

// Default secrets used only in defaults – startup fails if these are still active
const DEFAULT_ENCRYPTION_KEY: &str = "change-me-in-production-32bytes!";
const DEFAULT_JWT_SECRET: &str = "change-me-in-production";

/// Application configuration
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub telegram: TelegramConfig,
    pub detector: DetectorConfig,
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// Allowed CORS origins (comma-separated).
    /// In production set to your exact frontend domain, e.g. "https://fire.example.com".
    /// Empty list means CORS is disabled (suitable when nginx proxies same-origin).
    #[serde(default)]
    pub cors_origins: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub name: String,
    pub user: String,
    pub password: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub encryption_key: String,
}

impl DatabaseConfig {
    pub fn url(&self) -> String {
        std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            format!(
                "postgres://{}:{}@{}:{}/{}",
                self.user, self.password, self.host, self.port, self.name
            )
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub jwt_expiry_hours: u64,
    pub bcrypt_cost: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramConfig {
    pub enabled: bool,
    pub bot_token: String,
    pub default_chat_id: String,
    pub rate_limit_per_minute: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DetectorConfig {
    pub host: String,
    pub grpc_port: u16,
    /// Stream HTTP port (default: grpc_port + 1000)
    #[serde(default)]
    pub stream_port: Option<u16>,
}

impl DetectorConfig {
    pub fn stream_port(&self) -> u16 {
        self.stream_port.unwrap_or(self.grpc_port + 1000)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub snapshot_path: String,
    pub retention_days: u32,
}

impl AppConfig {
    /// Load configuration from files and environment
    pub fn load() -> Result<Self> {
        let config_dir = std::env::var("CONFIG_DIR")
            .unwrap_or_else(|_| "../../configs".to_string());

        let settings = config::Config::builder()
            .add_source(config::File::with_name(&format!("{}/settings", config_dir)))
            .add_source(config::Environment::with_prefix("FIRE_DETECT").separator("__"))
            .build()?;

        let config: Self = settings.try_deserialize()?;
        config.validate_secrets()?;
        Ok(config)
    }

    /// Fail fast if default/weak secrets are still in use.
    fn validate_secrets(&self) -> Result<()> {
        if self.database.encryption_key == DEFAULT_ENCRYPTION_KEY {
            anyhow::bail!(
                "FATAL: database.encryption_key is still the default placeholder. \
                 Set FIRE_DETECT__DATABASE__ENCRYPTION_KEY to a random 32-char secret."
            );
        }
        if self.database.encryption_key.len() < 32 {
            anyhow::bail!(
                "FATAL: database.encryption_key must be at least 32 characters \
                 (got {}). Shorter keys are zero-padded which reduces security.",
                self.database.encryption_key.len()
            );
        }
        if self.auth.jwt_secret == DEFAULT_JWT_SECRET {
            anyhow::bail!(
                "FATAL: auth.jwt_secret is still the default placeholder. \
                 Set FIRE_DETECT__AUTH__JWT_SECRET to a long random secret."
            );
        }
        if self.auth.jwt_secret.len() < 32 {
            anyhow::bail!(
                "FATAL: auth.jwt_secret must be at least 32 characters (got {}).",
                self.auth.jwt_secret.len()
            );
        }
        Ok(())
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            cors_origins: Vec::new(),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5432,
            name: "fire_detect".to_string(),
            user: "postgres".to_string(),
            password: "postgres".to_string(),
            max_connections: 10,
            min_connections: 2,
            encryption_key: "change-me-in-production-32bytes!".to_string(),
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: "change-me-in-production".to_string(),
            jwt_expiry_hours: 24,
            bcrypt_cost: 12,
        }
    }
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bot_token: String::new(),
            default_chat_id: String::new(),
            rate_limit_per_minute: 10,
        }
    }
}
