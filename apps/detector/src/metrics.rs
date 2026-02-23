//! Metrics module for Prometheus-compatible metrics
//!
//! Exposes metrics for monitoring detector performance.

use std::net::SocketAddr;
use std::time::Instant;
use metrics::{counter, gauge, histogram, describe_counter, describe_gauge, describe_histogram};
use metrics_exporter_prometheus::PrometheusBuilder;
use tokio::net::TcpListener;
use tracing::{error, info};

use crate::config::MetricsConfig;

/// Metrics server
pub struct MetricsServer;

impl MetricsServer {
    /// Run the metrics server
    pub async fn run(config: MetricsConfig) -> anyhow::Result<()> {
        if !config.enabled {
            return Ok(());
        }
        
        // Initialize Prometheus exporter
        let builder = PrometheusBuilder::new();
        
        let _handle = builder
            .with_http_listener(SocketAddr::from(([0, 0, 0, 0], config.port)))
            .install()
            .map_err(|e| anyhow::anyhow!("Failed to install Prometheus exporter: {}", e))?;
        
        // Describe metrics
        Self::describe_metrics();
        
        info!(port = config.port, "Metrics server started");
        
        // Keep running
        tokio::signal::ctrl_c().await?;
        
        Ok(())
    }
    
    /// Describe all metrics
    fn describe_metrics() {
        // Counters
        describe_counter!(
            "detector_frames_received_total",
            "Total frames received from cameras"
        );
        describe_counter!(
            "detector_frames_processed_total",
            "Total frames processed by inference"
        );
        describe_counter!(
            "detector_events_total",
            "Total detection events by type"
        );
        describe_counter!(
            "detector_errors_total",
            "Total errors by type"
        );
        
        // Gauges
        describe_gauge!(
            "detector_cameras_active",
            "Number of active camera streams"
        );
        describe_gauge!(
            "detector_cameras_failed",
            "Number of failed camera streams"
        );
        describe_gauge!(
            "detector_queue_size",
            "Current queue size per camera"
        );
        describe_gauge!(
            "detector_fps_in",
            "Input FPS per camera"
        );
        describe_gauge!(
            "detector_fps_infer",
            "Inference FPS per camera"
        );
        
        // Histograms
        describe_histogram!(
            "detector_inference_duration_seconds",
            "Inference duration in seconds"
        );
        describe_histogram!(
            "detector_preprocess_duration_seconds",
            "Preprocessing duration in seconds"
        );
        describe_histogram!(
            "detector_postprocess_duration_seconds",
            "Postprocessing duration in seconds"
        );
    }
}

/// Record a frame received
pub fn record_frame_received(camera_id: &str) {
    counter!("detector_frames_received_total", "camera_id" => camera_id.to_string()).increment(1);
}

/// Record a frame processed
pub fn record_frame_processed(camera_id: &str) {
    counter!("detector_frames_processed_total", "camera_id" => camera_id.to_string()).increment(1);
}

/// Record an event
pub fn record_event(camera_id: &str, event_type: &str) {
    counter!(
        "detector_events_total",
        "camera_id" => camera_id.to_string(),
        "event_type" => event_type.to_string()
    ).increment(1);
}

/// Record an error
pub fn record_error(camera_id: &str, error_type: &str) {
    counter!(
        "detector_errors_total",
        "camera_id" => camera_id.to_string(),
        "error_type" => error_type.to_string()
    ).increment(1);
}

/// Update active cameras
pub fn set_cameras_active(count: usize) {
    gauge!("detector_cameras_active").set(count as f64);
}

/// Update failed cameras
pub fn set_cameras_failed(count: usize) {
    gauge!("detector_cameras_failed").set(count as f64);
}

/// Update FPS metrics
pub fn set_fps(camera_id: &str, fps_in: f32, fps_infer: f32) {
    gauge!("detector_fps_in", "camera_id" => camera_id.to_string()).set(fps_in as f64);
    gauge!("detector_fps_infer", "camera_id" => camera_id.to_string()).set(fps_infer as f64);
}

/// Record inference duration
pub fn record_inference_duration(duration_secs: f64) {
    histogram!("detector_inference_duration_seconds").record(duration_secs);
}

/// Record preprocess duration
pub fn record_preprocess_duration(duration_secs: f64) {
    histogram!("detector_preprocess_duration_seconds").record(duration_secs);
}

/// Record postprocess duration
pub fn record_postprocess_duration(duration_secs: f64) {
    histogram!("detector_postprocess_duration_seconds").record(duration_secs);
}

/// Timer for measuring durations
pub struct Timer {
    start: Instant,
    metric_name: &'static str,
}

impl Timer {
    pub fn new(metric_name: &'static str) -> Self {
        Self {
            start: Instant::now(),
            metric_name,
        }
    }
    
    pub fn elapsed_secs(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let duration = self.elapsed_secs();
        histogram!(self.metric_name).record(duration);
    }
}
