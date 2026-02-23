//! Event module for publishing detection events
//!
//! Handles event creation, snapshot saving, and publishing to API service.

mod publisher;
mod models;
pub mod detector_grpc;

pub use publisher::EventPublisher;
pub use models::{EventType, FireEvent};
