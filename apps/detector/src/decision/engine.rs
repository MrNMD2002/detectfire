//! Decision engine
//!
//! Combines sliding window voting with cooldown logic to make detection decisions.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use parking_lot::RwLock;
use tracing::{debug, info};

use crate::config::CameraConfig;
use crate::inference::{Detection, DetectionClass, InferenceResult};

use super::cooldown::{CooldownEventType, CooldownManager};
use super::window::{FrameResult, SlidingWindow};

/// Event emitted by the decision engine
#[derive(Debug, Clone)]
pub struct DecisionEvent {
    /// Event type
    pub event_type: DecisionEventType,
    
    /// Camera ID
    pub camera_id: String,
    
    /// Site ID
    pub site_id: String,
    
    /// Confidence score
    pub confidence: f32,
    
    /// Detections that triggered the event
    pub detections: Vec<Detection>,
}

/// Type of decision event
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionEventType {
    Fire,
    Smoke,
}

impl std::fmt::Display for DecisionEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fire => write!(f, "fire"),
            Self::Smoke => write!(f, "smoke"),
        }
    }
}

/// Per-camera state for decision making
struct CameraState {
    /// Sliding window
    window: SlidingWindow,
    
    /// Last known site_id
    site_id: String,
    
    /// Configuration snapshot
    window_size: usize,
    fire_hits: usize,
    smoke_hits: usize,
    conf_fire: f32,
    conf_smoke: f32,
}

impl CameraState {
    fn new(config: &CameraConfig) -> Self {
        Self {
            window: SlidingWindow::new(config.window_size),
            site_id: config.site_id.clone(),
            window_size: config.window_size,
            fire_hits: config.fire_hits,
            smoke_hits: config.smoke_hits,
            conf_fire: config.conf_fire,
            conf_smoke: config.conf_smoke,
        }
    }
}

/// Decision engine for fire/smoke detection
#[derive(Clone)]
pub struct DecisionEngine {
    inner: Arc<DecisionEngineInner>,
}

struct DecisionEngineInner {
    /// Per-camera states
    camera_states: RwLock<HashMap<String, CameraState>>,
    
    /// Cooldown manager
    cooldown: CooldownManager,
}

impl DecisionEngine {
    /// Create a new decision engine
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DecisionEngineInner {
                camera_states: RwLock::new(HashMap::new()),
                cooldown: CooldownManager::new(),
            }),
        }
    }
    
    /// Create with custom cooldown configuration
    pub fn with_cooldowns(fire_cooldown_sec: u64, smoke_cooldown_sec: u64) -> Self {
        Self {
            inner: Arc::new(DecisionEngineInner {
                camera_states: RwLock::new(HashMap::new()),
                cooldown: CooldownManager::with_defaults(
                    Duration::from_secs(fire_cooldown_sec),
                    Duration::from_secs(smoke_cooldown_sec),
                    Duration::from_secs(30),
                    Duration::from_secs(5),
                ),
            }),
        }
    }
    
    /// Process inference result and potentially emit an event
    pub async fn process(
        &self,
        camera_id: &str,
        result: InferenceResult,
        config: &CameraConfig,
    ) -> Option<DecisionEvent> {
        // Filter detections by confidence
        let filtered_detections: Vec<Detection> = result.detections
            .into_iter()
            .filter(|d| d.is_above_threshold(config.conf_fire, config.conf_smoke, config.conf_other))
            .collect();
        
        // Create frame result
        let frame_result = FrameResult::from_detections(filtered_detections);
        
        // Get or create camera state
        {
            let mut states = self.inner.camera_states.write();
            if !states.contains_key(camera_id) {
                states.insert(camera_id.to_string(), CameraState::new(config));
            }
            
            // Update window
            if let Some(state) = states.get_mut(camera_id) {
                // Update config if changed
                if state.window_size != config.window_size {
                    state.window = SlidingWindow::new(config.window_size);
                    state.window_size = config.window_size;
                }
                state.fire_hits = config.fire_hits;
                state.smoke_hits = config.smoke_hits;
                state.conf_fire = config.conf_fire;
                state.conf_smoke = config.conf_smoke;
                state.site_id = config.site_id.clone();
                
                state.window.push(frame_result);
            }
        }
        
        // Check for events
        self.check_for_events(camera_id, config).await
    }
    
    /// Check if current window state triggers an event
    async fn check_for_events(
        &self,
        camera_id: &str,
        config: &CameraConfig,
    ) -> Option<DecisionEvent> {
        let states = self.inner.camera_states.read();
        let state = states.get(camera_id)?;
        
        // Check if window is sufficiently filled
        if !state.window.is_full() && state.window.fill_ratio() < 0.5 {
            return None; // Wait for more data
        }
        
        // Check for fire
        if state.window.fire_count() >= state.fire_hits {
            let cooldown_sec = Duration::from_secs(config.cooldown_sec);
            
            if self.inner.cooldown.try_fire_with_cooldown(
                camera_id,
                CooldownEventType::Fire,
                cooldown_sec,
            ) {
                let confidence = state.window.avg_fire_confidence();
                let detections = state.window.latest_detections()
                    .into_iter()
                    .filter(|d| d.class == DetectionClass::Fire)
                    .collect();
                
                info!(
                    camera_id = %camera_id,
                    fire_count = state.window.fire_count(),
                    threshold = state.fire_hits,
                    confidence = confidence,
                    "Fire event triggered"
                );
                
                return Some(DecisionEvent {
                    event_type: DecisionEventType::Fire,
                    camera_id: camera_id.to_string(),
                    site_id: state.site_id.clone(),
                    confidence,
                    detections,
                });
            } else {
                debug!(
                    camera_id = %camera_id,
                    "Fire detection in cooldown"
                );
            }
        }
        
        // Check for smoke
        if state.window.smoke_count() >= state.smoke_hits {
            let cooldown_sec = Duration::from_secs(config.cooldown_sec);
            
            if self.inner.cooldown.try_fire_with_cooldown(
                camera_id,
                CooldownEventType::Smoke,
                cooldown_sec,
            ) {
                let confidence = state.window.avg_smoke_confidence();
                let detections = state.window.latest_detections()
                    .into_iter()
                    .filter(|d| d.class == DetectionClass::Smoke)
                    .collect();
                
                info!(
                    camera_id = %camera_id,
                    smoke_count = state.window.smoke_count(),
                    threshold = state.smoke_hits,
                    confidence = confidence,
                    "Smoke event triggered"
                );
                
                return Some(DecisionEvent {
                    event_type: DecisionEventType::Smoke,
                    camera_id: camera_id.to_string(),
                    site_id: state.site_id.clone(),
                    confidence,
                    detections,
                });
            } else {
                debug!(
                    camera_id = %camera_id,
                    "Smoke detection in cooldown"
                );
            }
        }
        
        None
    }
    
    /// Get window statistics for a camera
    pub fn get_stats(&self, camera_id: &str) -> Option<DecisionStats> {
        let states = self.inner.camera_states.read();
        let state = states.get(camera_id)?;
        
        Some(DecisionStats {
            window_size: state.window.size(),
            window_fill: state.window.len(),
            fire_count: state.window.fire_count(),
            smoke_count: state.window.smoke_count(),
            fire_threshold: state.fire_hits,
            smoke_threshold: state.smoke_hits,
            avg_fire_confidence: state.window.avg_fire_confidence(),
            avg_smoke_confidence: state.window.avg_smoke_confidence(),
        })
    }
    
    /// Clear state for a camera
    pub fn clear_camera(&self, camera_id: &str) {
        self.inner.camera_states.write().remove(camera_id);
        self.inner.cooldown.clear_camera(camera_id);
    }
    
    /// Clear all state
    pub fn clear_all(&self) {
        self.inner.camera_states.write().clear();
        self.inner.cooldown.clear_all();
    }
}

impl Default for DecisionEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for decision engine state
#[derive(Debug, Clone)]
pub struct DecisionStats {
    pub window_size: usize,
    pub window_fill: usize,
    pub fire_count: usize,
    pub smoke_count: usize,
    pub fire_threshold: usize,
    pub smoke_threshold: usize,
    pub avg_fire_confidence: f32,
    pub avg_smoke_confidence: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::{BoundingBox, Detection};

    fn make_test_config() -> CameraConfig {
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
            window_size: 5,
            fire_hits: 3,
            smoke_hits: 3,
            cooldown_sec: 60,
        }
    }

    fn make_fire_result(conf: f32) -> InferenceResult {
        InferenceResult {
            detections: vec![Detection::new(
                DetectionClass::Fire,
                conf,
                BoundingBox::new(0.1, 0.1, 0.2, 0.2),
            )],
            ..Default::default()
        }
    }

    fn make_empty_result() -> InferenceResult {
        InferenceResult::empty()
    }

    #[tokio::test]
    async fn test_decision_engine_no_event_below_threshold() {
        let engine = DecisionEngine::new();
        let config = make_test_config();
        
        // Add only 2 fire detections (threshold is 3)
        engine.process("cam-01", make_fire_result(0.8), &config).await;
        engine.process("cam-01", make_fire_result(0.9), &config).await;
        engine.process("cam-01", make_empty_result(), &config).await;
        engine.process("cam-01", make_empty_result(), &config).await;
        let event = engine.process("cam-01", make_empty_result(), &config).await;
        
        // Should not trigger event
        assert!(event.is_none());
    }

    #[tokio::test]
    async fn test_decision_engine_fire_event() {
        let engine = DecisionEngine::new();
        let config = make_test_config();
        
        // Add 3 fire detections (threshold is 3)
        engine.process("cam-01", make_fire_result(0.8), &config).await;
        engine.process("cam-01", make_fire_result(0.9), &config).await;
        engine.process("cam-01", make_fire_result(0.85), &config).await;
        engine.process("cam-01", make_empty_result(), &config).await;
        let event = engine.process("cam-01", make_empty_result(), &config).await;
        
        // Should trigger fire event
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.event_type, DecisionEventType::Fire);
        assert_eq!(event.camera_id, "cam-01");
    }

    #[tokio::test]
    async fn test_decision_engine_cooldown() {
        let engine = DecisionEngine::with_cooldowns(1, 1); // 1 second cooldown
        let config = make_test_config();
        
        // Trigger first event
        for _ in 0..5 {
            engine.process("cam-01", make_fire_result(0.9), &config).await;
        }
        
        // Try to trigger again immediately (should be in cooldown)
        let event = engine.process("cam-01", make_fire_result(0.9), &config).await;
        assert!(event.is_none());
    }

    #[tokio::test]
    async fn test_decision_engine_stats() {
        let engine = DecisionEngine::new();
        let config = make_test_config();
        
        engine.process("cam-01", make_fire_result(0.8), &config).await;
        engine.process("cam-01", make_fire_result(0.9), &config).await;
        
        let stats = engine.get_stats("cam-01");
        assert!(stats.is_some());
        
        let stats = stats.unwrap();
        assert_eq!(stats.fire_count, 2);
        assert_eq!(stats.smoke_count, 0);
        assert!(stats.avg_fire_confidence > 0.8);
    }
}
