//! Frame sampler
//!
//! Samples frames at a configurable rate to reduce processing load.

use std::time::{Duration, Instant};

/// Frame sampler that ensures consistent frame rate
#[derive(Debug)]
pub struct FrameSampler {
    /// Target frames per second
    target_fps: f32,
    
    /// Minimum interval between frames
    min_interval: Duration,
    
    /// Last frame timestamp
    last_frame: Option<Instant>,
    
    /// Total frames received
    total_received: u64,
    
    /// Total frames sampled (passed through)
    total_sampled: u64,
}

impl FrameSampler {
    /// Create a new frame sampler with target FPS
    pub fn new(target_fps: u32) -> Self {
        let fps = target_fps.max(1) as f32;
        let min_interval = Duration::from_secs_f32(1.0 / fps);
        
        Self {
            target_fps: fps,
            min_interval,
            last_frame: None,
            total_received: 0,
            total_sampled: 0,
        }
    }
    
    /// Check if a frame should be sampled
    /// 
    /// Returns `true` if enough time has passed since the last sampled frame
    pub fn should_sample(&mut self) -> bool {
        self.total_received += 1;
        
        let now = Instant::now();
        
        match self.last_frame {
            Some(last) => {
                let elapsed = now.duration_since(last);
                if elapsed >= self.min_interval {
                    self.last_frame = Some(now);
                    self.total_sampled += 1;
                    true
                } else {
                    false
                }
            }
            None => {
                // First frame, always sample
                self.last_frame = Some(now);
                self.total_sampled += 1;
                true
            }
        }
    }
    
    /// Get the target FPS
    pub fn target_fps(&self) -> f32 {
        self.target_fps
    }
    
    /// Get total frames received
    pub fn total_received(&self) -> u64 {
        self.total_received
    }
    
    /// Get total frames sampled
    pub fn total_sampled(&self) -> u64 {
        self.total_sampled
    }
    
    /// Get sample ratio (sampled / received)
    pub fn sample_ratio(&self) -> f32 {
        if self.total_received == 0 {
            0.0
        } else {
            self.total_sampled as f32 / self.total_received as f32
        }
    }
    
    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.total_received = 0;
        self.total_sampled = 0;
    }
    
    /// Update target FPS
    pub fn set_target_fps(&mut self, fps: u32) {
        self.target_fps = fps.max(1) as f32;
        self.min_interval = Duration::from_secs_f32(1.0 / self.target_fps);
    }
}

/// Adaptive frame sampler that adjusts based on processing load
#[derive(Debug)]
pub struct AdaptiveFrameSampler {
    /// Base sampler
    sampler: FrameSampler,
    
    /// Target FPS range
    min_fps: f32,
    max_fps: f32,
    
    /// Processing time history
    processing_times: Vec<Duration>,
    
    /// Max processing times to track
    history_size: usize,
    
    /// Target processing time threshold
    target_latency: Duration,
}

impl AdaptiveFrameSampler {
    /// Create a new adaptive frame sampler
    pub fn new(min_fps: u32, max_fps: u32, target_latency_ms: u64) -> Self {
        let initial_fps = (min_fps + max_fps) / 2;
        
        Self {
            sampler: FrameSampler::new(initial_fps),
            min_fps: min_fps as f32,
            max_fps: max_fps as f32,
            processing_times: Vec::with_capacity(50),
            history_size: 50,
            target_latency: Duration::from_millis(target_latency_ms),
        }
    }
    
    /// Check if a frame should be sampled
    pub fn should_sample(&mut self) -> bool {
        self.sampler.should_sample()
    }
    
    /// Record processing time for adaptation
    pub fn record_processing_time(&mut self, duration: Duration) {
        self.processing_times.push(duration);
        
        // Trim old entries
        if self.processing_times.len() > self.history_size {
            self.processing_times.remove(0);
        }
        
        // Adapt FPS based on average processing time
        self.adapt();
    }
    
    /// Adapt FPS based on processing load
    fn adapt(&mut self) {
        if self.processing_times.len() < 10 {
            return; // Not enough data
        }
        
        // Calculate average processing time
        let total: Duration = self.processing_times.iter().sum();
        let avg = total / self.processing_times.len() as u32;
        
        let current_fps = self.sampler.target_fps();
        let new_fps: f32;
        
        if avg > self.target_latency {
            // Processing too slow, reduce FPS
            new_fps = (current_fps * 0.9).max(self.min_fps);
        } else if avg < self.target_latency / 2 {
            // Processing fast, can increase FPS
            new_fps = (current_fps * 1.1).min(self.max_fps);
        } else {
            return; // In acceptable range
        }
        
        if (new_fps - current_fps).abs() > 0.5 {
            self.sampler.set_target_fps(new_fps as u32);
        }
    }
    
    /// Get current target FPS
    pub fn current_fps(&self) -> f32 {
        self.sampler.target_fps()
    }
    
    /// Get base sampler statistics
    pub fn stats(&self) -> (u64, u64, f32) {
        (
            self.sampler.total_received(),
            self.sampler.total_sampled(),
            self.sampler.sample_ratio(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_frame_sampler_basic() {
        let mut sampler = FrameSampler::new(10); // 10 FPS = 100ms interval
        
        // First frame should always be sampled
        assert!(sampler.should_sample());
        
        // Immediate second frame should be skipped
        assert!(!sampler.should_sample());
        
        // Wait for interval
        thread::sleep(Duration::from_millis(110));
        assert!(sampler.should_sample());
    }

    #[test]
    fn test_frame_sampler_high_fps() {
        let mut sampler = FrameSampler::new(30); // 30 FPS = ~33ms interval
        
        let mut sampled = 0;
        let start = Instant::now();
        
        // Simulate high frame rate input
        while start.elapsed() < Duration::from_secs(1) {
            if sampler.should_sample() {
                sampled += 1;
            }
            thread::sleep(Duration::from_millis(10)); // 100 FPS input
        }
        
        // Should sample approximately 30 frames
        assert!(sampled >= 25 && sampled <= 35, "Expected ~30, got {}", sampled);
    }

    #[test]
    fn test_frame_sampler_stats() {
        let mut sampler = FrameSampler::new(5);
        
        for _ in 0..20 {
            sampler.should_sample();
            thread::sleep(Duration::from_millis(50));
        }
        
        assert_eq!(sampler.total_received(), 20);
        assert!(sampler.total_sampled() > 0);
        assert!(sampler.sample_ratio() <= 1.0);
    }

    #[test]
    fn test_adaptive_sampler_slow_processing() {
        let mut sampler = AdaptiveFrameSampler::new(2, 10, 50);
        
        let initial_fps = sampler.current_fps();
        
        // Simulate slow processing
        for _ in 0..20 {
            if sampler.should_sample() {
                sampler.record_processing_time(Duration::from_millis(100));
            }
        }
        
        // FPS should decrease
        assert!(sampler.current_fps() < initial_fps);
    }
}
