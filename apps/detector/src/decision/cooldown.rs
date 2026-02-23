//! Cooldown manager for rate limiting alerts
//!
//! Prevents alert spam by enforcing minimum time between events.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use parking_lot::RwLock;

/// Event type for cooldown tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CooldownEventType {
    Fire,
    Smoke,
    StreamDown,
    StreamUp,
}

impl std::fmt::Display for CooldownEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fire => write!(f, "fire"),
            Self::Smoke => write!(f, "smoke"),
            Self::StreamDown => write!(f, "stream_down"),
            Self::StreamUp => write!(f, "stream_up"),
        }
    }
}

/// Cooldown key combining camera and event type
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CooldownKey {
    camera_id: String,
    event_type: CooldownEventType,
}

/// Manages cooldown periods for events
#[derive(Debug)]
pub struct CooldownManager {
    /// Last event timestamps
    last_events: RwLock<HashMap<CooldownKey, Instant>>,
    
    /// Default cooldown durations per event type
    default_cooldowns: HashMap<CooldownEventType, Duration>,
}

impl CooldownManager {
    /// Create a new cooldown manager with default durations
    pub fn new() -> Self {
        let mut default_cooldowns = HashMap::new();
        
        // Default cooldowns
        default_cooldowns.insert(CooldownEventType::Fire, Duration::from_secs(60));
        default_cooldowns.insert(CooldownEventType::Smoke, Duration::from_secs(60));
        default_cooldowns.insert(CooldownEventType::StreamDown, Duration::from_secs(30));
        default_cooldowns.insert(CooldownEventType::StreamUp, Duration::from_secs(5));
        
        Self {
            last_events: RwLock::new(HashMap::new()),
            default_cooldowns,
        }
    }
    
    /// Create with custom default cooldowns
    pub fn with_defaults(
        fire_cooldown: Duration,
        smoke_cooldown: Duration,
        stream_down_cooldown: Duration,
        stream_up_cooldown: Duration,
    ) -> Self {
        let mut default_cooldowns = HashMap::new();
        
        default_cooldowns.insert(CooldownEventType::Fire, fire_cooldown);
        default_cooldowns.insert(CooldownEventType::Smoke, smoke_cooldown);
        default_cooldowns.insert(CooldownEventType::StreamDown, stream_down_cooldown);
        default_cooldowns.insert(CooldownEventType::StreamUp, stream_up_cooldown);
        
        Self {
            last_events: RwLock::new(HashMap::new()),
            default_cooldowns,
        }
    }
    
    /// Check if an event can be fired (not in cooldown)
    pub fn can_fire(&self, camera_id: &str, event_type: CooldownEventType) -> bool {
        self.can_fire_with_cooldown(
            camera_id,
            event_type,
            self.default_cooldowns.get(&event_type).copied().unwrap_or_default(),
        )
    }
    
    /// Check if an event can be fired with custom cooldown
    pub fn can_fire_with_cooldown(
        &self,
        camera_id: &str,
        event_type: CooldownEventType,
        cooldown: Duration,
    ) -> bool {
        let key = CooldownKey {
            camera_id: camera_id.to_string(),
            event_type,
        };
        
        let events = self.last_events.read();
        
        match events.get(&key) {
            Some(last_time) => {
                last_time.elapsed() >= cooldown
            }
            None => true,
        }
    }
    
    /// Record an event (start cooldown)
    pub fn record_event(&self, camera_id: &str, event_type: CooldownEventType) {
        let key = CooldownKey {
            camera_id: camera_id.to_string(),
            event_type,
        };
        
        self.last_events.write().insert(key, Instant::now());
    }
    
    /// Try to fire an event - checks cooldown and records if allowed
    /// 
    /// Returns true if event was fired, false if in cooldown
    pub fn try_fire(&self, camera_id: &str, event_type: CooldownEventType) -> bool {
        self.try_fire_with_cooldown(
            camera_id,
            event_type,
            self.default_cooldowns.get(&event_type).copied().unwrap_or_default(),
        )
    }
    
    /// Try to fire with custom cooldown
    pub fn try_fire_with_cooldown(
        &self,
        camera_id: &str,
        event_type: CooldownEventType,
        cooldown: Duration,
    ) -> bool {
        let key = CooldownKey {
            camera_id: camera_id.to_string(),
            event_type,
        };
        
        let mut events = self.last_events.write();
        
        let can_fire = match events.get(&key) {
            Some(last_time) => last_time.elapsed() >= cooldown,
            None => true,
        };
        
        if can_fire {
            events.insert(key, Instant::now());
        }
        
        can_fire
    }
    
    /// Get remaining cooldown time
    pub fn remaining_cooldown(
        &self,
        camera_id: &str,
        event_type: CooldownEventType,
    ) -> Option<Duration> {
        self.remaining_cooldown_with_duration(
            camera_id,
            event_type,
            self.default_cooldowns.get(&event_type).copied().unwrap_or_default(),
        )
    }
    
    /// Get remaining cooldown with custom duration
    pub fn remaining_cooldown_with_duration(
        &self,
        camera_id: &str,
        event_type: CooldownEventType,
        cooldown: Duration,
    ) -> Option<Duration> {
        let key = CooldownKey {
            camera_id: camera_id.to_string(),
            event_type,
        };
        
        let events = self.last_events.read();
        
        match events.get(&key) {
            Some(last_time) => {
                let elapsed = last_time.elapsed();
                if elapsed < cooldown {
                    Some(cooldown - elapsed)
                } else {
                    None
                }
            }
            None => None,
        }
    }
    
    /// Clear cooldown for a specific event
    pub fn clear(&self, camera_id: &str, event_type: CooldownEventType) {
        let key = CooldownKey {
            camera_id: camera_id.to_string(),
            event_type,
        };
        
        self.last_events.write().remove(&key);
    }
    
    /// Clear all cooldowns for a camera
    pub fn clear_camera(&self, camera_id: &str) {
        self.last_events
            .write()
            .retain(|k, _| k.camera_id != camera_id);
    }
    
    /// Clear all cooldowns
    pub fn clear_all(&self) {
        self.last_events.write().clear();
    }
    
    /// Get default cooldown for event type
    pub fn default_cooldown(&self, event_type: CooldownEventType) -> Duration {
        self.default_cooldowns.get(&event_type).copied().unwrap_or_default()
    }
    
    /// Set default cooldown for event type
    pub fn set_default_cooldown(&mut self, event_type: CooldownEventType, duration: Duration) {
        self.default_cooldowns.insert(event_type, duration);
    }
}

impl Default for CooldownManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_cooldown_basic() {
        let manager = CooldownManager::new();
        
        // First event should be allowed
        assert!(manager.can_fire("cam-01", CooldownEventType::Fire));
        
        // Record event
        manager.record_event("cam-01", CooldownEventType::Fire);
        
        // Immediate second event should be blocked
        assert!(!manager.can_fire("cam-01", CooldownEventType::Fire));
    }

    #[test]
    fn test_cooldown_different_cameras() {
        let manager = CooldownManager::new();
        
        manager.record_event("cam-01", CooldownEventType::Fire);
        
        // Different camera should be allowed
        assert!(manager.can_fire("cam-02", CooldownEventType::Fire));
    }

    #[test]
    fn test_cooldown_different_event_types() {
        let manager = CooldownManager::new();
        
        manager.record_event("cam-01", CooldownEventType::Fire);
        
        // Different event type should be allowed
        assert!(manager.can_fire("cam-01", CooldownEventType::Smoke));
    }

    #[test]
    fn test_cooldown_expiry() {
        let manager = CooldownManager::with_defaults(
            Duration::from_millis(50),
            Duration::from_millis(50),
            Duration::from_millis(50),
            Duration::from_millis(50),
        );
        
        manager.record_event("cam-01", CooldownEventType::Fire);
        
        // Should be blocked
        assert!(!manager.can_fire("cam-01", CooldownEventType::Fire));
        
        // Wait for expiry
        thread::sleep(Duration::from_millis(60));
        
        // Should be allowed
        assert!(manager.can_fire("cam-01", CooldownEventType::Fire));
    }

    #[test]
    fn test_try_fire() {
        let manager = CooldownManager::with_defaults(
            Duration::from_secs(60),
            Duration::from_secs(60),
            Duration::from_secs(30),
            Duration::from_secs(5),
        );
        
        // First try should succeed
        assert!(manager.try_fire("cam-01", CooldownEventType::Fire));
        
        // Second try should fail (in cooldown)
        assert!(!manager.try_fire("cam-01", CooldownEventType::Fire));
    }

    #[test]
    fn test_remaining_cooldown() {
        let manager = CooldownManager::with_defaults(
            Duration::from_secs(60),
            Duration::from_secs(60),
            Duration::from_secs(30),
            Duration::from_secs(5),
        );
        
        // No cooldown initially
        assert!(manager.remaining_cooldown("cam-01", CooldownEventType::Fire).is_none());
        
        // Record event
        manager.record_event("cam-01", CooldownEventType::Fire);
        
        // Should have remaining cooldown
        let remaining = manager.remaining_cooldown("cam-01", CooldownEventType::Fire);
        assert!(remaining.is_some());
        assert!(remaining.unwrap() < Duration::from_secs(60));
    }

    #[test]
    fn test_clear_cooldown() {
        let manager = CooldownManager::new();
        
        manager.record_event("cam-01", CooldownEventType::Fire);
        assert!(!manager.can_fire("cam-01", CooldownEventType::Fire));
        
        manager.clear("cam-01", CooldownEventType::Fire);
        assert!(manager.can_fire("cam-01", CooldownEventType::Fire));
    }
}
