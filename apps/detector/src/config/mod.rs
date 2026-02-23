//! Configuration module for detector service
//!
//! Handles loading and validation of camera and global settings.

mod loader;
mod models;
mod validation;

pub use loader::{AppConfig, GlobalConfig};
pub use models::*;
pub use validation::validate_config;
