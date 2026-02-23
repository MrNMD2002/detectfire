//! Camera status tracking
//!
//! Tracks the state and metrics of each camera stream.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Stream state enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamState {
    /// Initial state, not yet connected
    Unknown,
    /// Attempting to connect
    Connecting,
    /// Connected but not yet streaming
    Connected,
    /// Actively receiving frames
    Streaming,
    /// Lost connection, attempting to reconnect
    Reconnecting,
    /// Max retries exceeded, manual intervention needed
    Failed,
    /// Camera is disabled in config
    Disabled,
}

impl Default for StreamState {
    fn default() -> Self {
        Self::Unknown
    }
}

impl std::fmt::Display for StreamState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => write!(f, "unknown"),
            Self::Connecting => write!(f, "connecting"),
            Self::Connected => write!(f, "connected"),
            Self::Streaming => write!(f, "streaming"),
            Self::Reconnecting => write!(f, "reconnecting"),
            Self::Failed => write!(f, "failed"),
            Self::Disabled => write!(f, "disabled"),
        }
    }
}

/// Camera status with thread-safe access
#[derive(Debug)]
pub struct CameraStatus {
    /// Camera identifier
    pub camera_id: String,
    
    /// Site identifier
    pub site_id: String,
    
    /// Camera name
    pub name: String,
    
    /// Current stream state
    state: RwLock<StreamState>,
    
    /// Number of reconnection attempts
    reconnect_count: AtomicU32,
    
    /// Last error message
    last_error: RwLock<Option<String>>,
    
    /// FPS metrics
    fps_tracker: Arc<FpsTracker>,
    
    /// Last frame timestamp (unix millis)
    last_frame_ts: AtomicU64,
    
    /// Time when stream started
    stream_started: RwLock<Option<Instant>>,
}

impl CameraStatus {
    /// Create a new camera status
    pub fn new(camera_id: &str, site_id: &str, name: &str) -> Self {
        Self {
            camera_id: camera_id.to_string(),
            site_id: site_id.to_string(),
            name: name.to_string(),
            state: RwLock::new(StreamState::Unknown),
            reconnect_count: AtomicU32::new(0),
            last_error: RwLock::new(None),
            fps_tracker: Arc::new(FpsTracker::new()),
            last_frame_ts: AtomicU64::new(0),
            stream_started: RwLock::new(None),
        }
    }
    
    /// Get current state
    pub fn state(&self) -> StreamState {
        *self.state.read()
    }
    
    /// Set current state
    pub fn set_state(&self, state: StreamState) {
        let mut guard = self.state.write();
        let old_state = *guard;
        *guard = state;
        
        // Track stream start time
        if state == StreamState::Streaming && old_state != StreamState::Streaming {
            *self.stream_started.write() = Some(Instant::now());
        }
        
        // Reset reconnect count on successful stream
        if state == StreamState::Streaming {
            self.reconnect_count.store(0, Ordering::Relaxed);
        }
    }
    
    /// Get reconnection count
    pub fn reconnect_count(&self) -> u32 {
        self.reconnect_count.load(Ordering::Relaxed)
    }
    
    /// Increment reconnection count
    pub fn increment_reconnect(&self) -> u32 {
        self.reconnect_count.fetch_add(1, Ordering::Relaxed) + 1
    }
    
    /// Reset reconnection count
    pub fn reset_reconnect(&self) {
        self.reconnect_count.store(0, Ordering::Relaxed);
    }
    
    /// Set last error
    pub fn set_error(&self, error: Option<String>) {
        *self.last_error.write() = error;
    }
    
    /// Get last error
    pub fn last_error(&self) -> Option<String> {
        self.last_error.read().clone()
    }
    
    /// Record a frame received
    pub fn record_frame(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        self.last_frame_ts.store(now, Ordering::Relaxed);
        self.fps_tracker.record_frame();
    }
    
    /// Record inference completed
    pub fn record_inference(&self) {
        self.fps_tracker.record_inference();
    }
    
    /// Get input FPS (frames received)
    pub fn fps_in(&self) -> f32 {
        self.fps_tracker.fps_in()
    }
    
    /// Get inference FPS (frames processed)
    pub fn fps_infer(&self) -> f32 {
        self.fps_tracker.fps_infer()
    }
    
    /// Get last frame timestamp
    pub fn last_frame_timestamp(&self) -> u64 {
        self.last_frame_ts.load(Ordering::Relaxed)
    }
    
    /// Get stream uptime
    pub fn uptime(&self) -> Option<Duration> {
        self.stream_started.read().map(|start| start.elapsed())
    }
    
    /// Check if stream is healthy (receiving frames)
    pub fn is_healthy(&self) -> bool {
        matches!(self.state(), StreamState::Streaming) && self.fps_in() > 0.5
    }
    
    /// Create a snapshot of current status for reporting
    pub fn snapshot(&self) -> CameraStatusSnapshot {
        CameraStatusSnapshot {
            camera_id: self.camera_id.clone(),
            site_id: self.site_id.clone(),
            name: self.name.clone(),
            state: self.state(),
            reconnect_count: self.reconnect_count(),
            fps_in: self.fps_in(),
            fps_infer: self.fps_infer(),
            last_frame_ts: self.last_frame_timestamp(),
            last_error: self.last_error(),
        }
    }
}

/// Serializable snapshot of camera status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraStatusSnapshot {
    pub camera_id: String,
    pub site_id: String,
    pub name: String,
    pub state: StreamState,
    pub reconnect_count: u32,
    pub fps_in: f32,
    pub fps_infer: f32,
    pub last_frame_ts: u64,
    pub last_error: Option<String>,
}

/// FPS tracking with sliding window.
///
/// Uses `VecDeque` so old entries are removed from the front in O(1) instead of
/// scanning the whole buffer with `retain()` on every frame.
#[derive(Debug)]
struct FpsTracker {
    /// Frame timestamps for input FPS calculation
    frame_times: RwLock<VecDeque<Instant>>,

    /// Inference timestamps for inference FPS calculation
    infer_times: RwLock<VecDeque<Instant>>,

    /// Window duration for FPS calculation
    window: Duration,
}

impl FpsTracker {
    fn new() -> Self {
        Self {
            frame_times: RwLock::new(VecDeque::with_capacity(100)),
            infer_times: RwLock::new(VecDeque::with_capacity(100)),
            window: Duration::from_secs(5),
        }
    }

    fn record_frame(&self) {
        let now = Instant::now();
        let cutoff = now - self.window;
        let mut times = self.frame_times.write();
        // Pop stale entries from the front (O(1) per entry)
        while times.front().map(|t| *t <= cutoff).unwrap_or(false) {
            times.pop_front();
        }
        times.push_back(now);
    }

    fn record_inference(&self) {
        let now = Instant::now();
        let cutoff = now - self.window;
        let mut times = self.infer_times.write();
        while times.front().map(|t| *t <= cutoff).unwrap_or(false) {
            times.pop_front();
        }
        times.push_back(now);
    }

    fn fps_in(&self) -> f32 {
        self.calculate_fps(&self.frame_times.read())
    }

    fn fps_infer(&self) -> f32 {
        self.calculate_fps(&self.infer_times.read())
    }

    fn calculate_fps(&self, times: &VecDeque<Instant>) -> f32 {
        if times.len() < 2 {
            return 0.0;
        }

        let now = Instant::now();
        let cutoff = now - self.window;
        // Front entries are already pruned during recording, but guard again in
        // case some time passed between record and calculate.
        let first = times.iter().find(|t| **t > cutoff);
        let last = times.back();

        let recent_count = times.iter().filter(|t| **t > cutoff).count();

        match (first, last) {
            (Some(f), Some(l)) if l > f => {
                let duration = l.duration_since(*f);
                if duration.as_secs_f32() < 0.001 {
                    return 0.0;
                }
                (recent_count.saturating_sub(1)) as f32 / duration.as_secs_f32()
            }
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_camera_status_state_transitions() {
        let status = CameraStatus::new("cam-01", "site-a", "Test Camera");
        
        assert_eq!(status.state(), StreamState::Unknown);
        
        status.set_state(StreamState::Connecting);
        assert_eq!(status.state(), StreamState::Connecting);
        
        status.set_state(StreamState::Streaming);
        assert_eq!(status.state(), StreamState::Streaming);
        assert_eq!(status.reconnect_count(), 0);
    }

    #[test]
    fn test_camera_status_reconnect_count() {
        let status = CameraStatus::new("cam-01", "site-a", "Test Camera");
        
        assert_eq!(status.reconnect_count(), 0);
        assert_eq!(status.increment_reconnect(), 1);
        assert_eq!(status.increment_reconnect(), 2);
        assert_eq!(status.reconnect_count(), 2);
        
        status.set_state(StreamState::Streaming);
        assert_eq!(status.reconnect_count(), 0);
    }

    #[test]
    fn test_fps_tracker() {
        let tracker = FpsTracker::new();
        
        // Record frames quickly
        for _ in 0..10 {
            tracker.record_frame();
            thread::sleep(Duration::from_millis(50));
        }
        
        let fps = tracker.fps_in();
        assert!(fps > 15.0 && fps < 25.0, "Expected ~20 FPS, got {}", fps);
    }
}
