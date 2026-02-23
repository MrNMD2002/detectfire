//! HLS stream module
//!
//! On-demand HLS streaming for web viewing. Streams only start when a client connects.

mod server;

pub use server::StreamServer;
