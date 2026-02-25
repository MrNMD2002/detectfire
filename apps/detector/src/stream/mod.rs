//! MJPEG stream module (MBFS-Stream approach)
//!
//! Live MJPEG streaming — subscribes to the inference pipeline broadcast channel
//! and encodes frames as JPEG on demand. No separate RTSP connection needed.

mod server;

pub use server::StreamServer;
