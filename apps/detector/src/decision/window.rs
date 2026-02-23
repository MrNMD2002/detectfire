//! Sliding window for detection voting
//!
//! Tracks recent detections to reduce false positives.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::inference::Detection;

/// A single frame result in the sliding window
#[derive(Debug, Clone)]
pub struct FrameResult {
    /// Timestamp when frame was processed
    pub timestamp: Instant,
    
    /// Whether fire was detected
    pub has_fire: bool,
    
    /// Whether smoke was detected
    pub has_smoke: bool,
    
    /// Best fire confidence (if any)
    pub fire_confidence: Option<f32>,
    
    /// Best smoke confidence (if any)
    pub smoke_confidence: Option<f32>,
    
    /// All detections
    pub detections: Vec<Detection>,
}

impl FrameResult {
    /// Create from detections
    pub fn from_detections(detections: Vec<Detection>) -> Self {
        let fire_detections: Vec<_> = detections
            .iter()
            .filter(|d| d.class == crate::inference::DetectionClass::Fire)
            .collect();
        
        let smoke_detections: Vec<_> = detections
            .iter()
            .filter(|d| d.class == crate::inference::DetectionClass::Smoke)
            .collect();
        
        let fire_confidence = fire_detections
            .iter()
            .map(|d| d.confidence)
            .max_by(|a, b| a.partial_cmp(b).unwrap());
        
        let smoke_confidence = smoke_detections
            .iter()
            .map(|d| d.confidence)
            .max_by(|a, b| a.partial_cmp(b).unwrap());
        
        Self {
            timestamp: Instant::now(),
            has_fire: !fire_detections.is_empty(),
            has_smoke: !smoke_detections.is_empty(),
            fire_confidence,
            smoke_confidence,
            detections,
        }
    }
    
    /// Create empty result (no detections)
    pub fn empty() -> Self {
        Self {
            timestamp: Instant::now(),
            has_fire: false,
            has_smoke: false,
            fire_confidence: None,
            smoke_confidence: None,
            detections: Vec::new(),
        }
    }
}

/// Sliding window buffer for detection voting
#[derive(Debug)]
pub struct SlidingWindow {
    /// Window size (number of frames to track)
    size: usize,
    
    /// Frame results buffer
    buffer: VecDeque<FrameResult>,
    
    /// Maximum age for entries
    max_age: Duration,
}

impl SlidingWindow {
    /// Create a new sliding window
    pub fn new(size: usize) -> Self {
        Self {
            size,
            buffer: VecDeque::with_capacity(size),
            max_age: Duration::from_secs(30), // Expire old entries after 30s
        }
    }
    
    /// Create with custom max age
    pub fn with_max_age(size: usize, max_age: Duration) -> Self {
        Self {
            size,
            buffer: VecDeque::with_capacity(size),
            max_age,
        }
    }
    
    /// Push a new frame result
    pub fn push(&mut self, result: FrameResult) {
        // Remove old entries
        self.prune_old();
        
        // Add new entry
        self.buffer.push_back(result);
        
        // Trim to size
        while self.buffer.len() > self.size {
            self.buffer.pop_front();
        }
    }
    
    /// Count fire detections in window
    pub fn fire_count(&self) -> usize {
        self.buffer.iter().filter(|r| r.has_fire).count()
    }
    
    /// Count smoke detections in window
    pub fn smoke_count(&self) -> usize {
        self.buffer.iter().filter(|r| r.has_smoke).count()
    }
    
    /// Get average fire confidence
    pub fn avg_fire_confidence(&self) -> f32 {
        let confidences: Vec<f32> = self.buffer
            .iter()
            .filter_map(|r| r.fire_confidence)
            .collect();
        
        if confidences.is_empty() {
            0.0
        } else {
            confidences.iter().sum::<f32>() / confidences.len() as f32
        }
    }
    
    /// Get average smoke confidence
    pub fn avg_smoke_confidence(&self) -> f32 {
        let confidences: Vec<f32> = self.buffer
            .iter()
            .filter_map(|r| r.smoke_confidence)
            .collect();
        
        if confidences.is_empty() {
            0.0
        } else {
            confidences.iter().sum::<f32>() / confidences.len() as f32
        }
    }
    
    /// Get max fire confidence in window
    pub fn max_fire_confidence(&self) -> f32 {
        self.buffer
            .iter()
            .filter_map(|r| r.fire_confidence)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0)
    }
    
    /// Get max smoke confidence in window
    pub fn max_smoke_confidence(&self) -> f32 {
        self.buffer
            .iter()
            .filter_map(|r| r.smoke_confidence)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0)
    }
    
    /// Get all detections from the most recent frame with detections
    pub fn latest_detections(&self) -> Vec<Detection> {
        self.buffer
            .iter()
            .rev()
            .find(|r| !r.detections.is_empty())
            .map(|r| r.detections.clone())
            .unwrap_or_default()
    }
    
    /// Get window fill ratio
    pub fn fill_ratio(&self) -> f32 {
        self.buffer.len() as f32 / self.size as f32
    }
    
    /// Check if window is full
    pub fn is_full(&self) -> bool {
        self.buffer.len() >= self.size
    }
    
    /// Clear the window
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
    
    /// Get window size
    pub fn size(&self) -> usize {
        self.size
    }
    
    /// Get current count
    pub fn len(&self) -> usize {
        self.buffer.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
    
    /// Prune old entries
    fn prune_old(&mut self) {
        let now = Instant::now();
        while let Some(front) = self.buffer.front() {
            if now.duration_since(front.timestamp) > self.max_age {
                self.buffer.pop_front();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::{Detection, DetectionClass, BoundingBox};

    fn make_fire_detection(conf: f32) -> Detection {
        Detection::new(
            DetectionClass::Fire,
            conf,
            BoundingBox::new(0.1, 0.1, 0.2, 0.2),
        )
    }

    fn make_smoke_detection(conf: f32) -> Detection {
        Detection::new(
            DetectionClass::Smoke,
            conf,
            BoundingBox::new(0.1, 0.1, 0.2, 0.2),
        )
    }

    #[test]
    fn test_sliding_window_basic() {
        let mut window = SlidingWindow::new(5);
        
        // Push some results
        window.push(FrameResult::from_detections(vec![make_fire_detection(0.8)]));
        window.push(FrameResult::from_detections(vec![make_fire_detection(0.9)]));
        window.push(FrameResult::empty());
        
        assert_eq!(window.len(), 3);
        assert_eq!(window.fire_count(), 2);
        assert_eq!(window.smoke_count(), 0);
    }

    #[test]
    fn test_sliding_window_overflow() {
        let mut window = SlidingWindow::new(3);
        
        // Push more than capacity
        for i in 0..5 {
            let conf = 0.5 + i as f32 * 0.1;
            window.push(FrameResult::from_detections(vec![make_fire_detection(conf)]));
        }
        
        // Should only have 3 entries
        assert_eq!(window.len(), 3);
        
        // Should have latest 3 (highest confidences)
        assert!((window.max_fire_confidence() - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_sliding_window_confidence_stats() {
        let mut window = SlidingWindow::new(5);
        
        window.push(FrameResult::from_detections(vec![make_fire_detection(0.7)]));
        window.push(FrameResult::from_detections(vec![make_fire_detection(0.8)]));
        window.push(FrameResult::from_detections(vec![make_fire_detection(0.9)]));
        
        assert!((window.avg_fire_confidence() - 0.8).abs() < 0.01);
        assert!((window.max_fire_confidence() - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_sliding_window_mixed_detections() {
        let mut window = SlidingWindow::new(5);
        
        window.push(FrameResult::from_detections(vec![make_fire_detection(0.8)]));
        window.push(FrameResult::from_detections(vec![make_smoke_detection(0.7)]));
        window.push(FrameResult::from_detections(vec![
            make_fire_detection(0.9),
            make_smoke_detection(0.85),
        ]));
        
        assert_eq!(window.fire_count(), 2);
        assert_eq!(window.smoke_count(), 2);
    }
}
