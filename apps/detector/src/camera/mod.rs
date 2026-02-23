//! Camera module for RTSP ingestion and frame processing
//!
//! This module handles:
//! - GStreamer pipeline setup for RTSP streams
//! - Frame sampling at configurable FPS
//! - Queue management with drop-old strategy
//! - Reconnection logic with exponential backoff

mod manager;
mod pipeline;
mod sampler;
mod worker;
mod status;

pub use manager::CameraManager;
pub use pipeline::{CameraPipeline, Frame};
pub use sampler::FrameSampler;
pub use worker::CameraWorker;
pub use status::{CameraStatus, StreamState};
