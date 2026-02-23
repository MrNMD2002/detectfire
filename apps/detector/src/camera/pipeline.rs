//! GStreamer pipeline for RTSP ingestion
//!
//! Creates optimized pipelines for RTSP streams with appsink for frame access.

use std::sync::Arc;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use tracing::{debug, error, info, warn};

use crate::config::CameraConfig;
use crate::error::DetectorError;

/// GStreamer pipeline for a single camera
pub struct CameraPipeline {
    /// Camera configuration
    config: CameraConfig,
    
    /// GStreamer pipeline
    pipeline: Option<gst::Pipeline>,
    
    /// App sink for frame access
    appsink: Option<gst_app::AppSink>,
    
    /// Pipeline state
    is_running: bool,
}

impl CameraPipeline {
    /// Create a new camera pipeline
    pub fn new(config: CameraConfig) -> Self {
        Self {
            config,
            pipeline: None,
            appsink: None,
            is_running: false,
        }
    }
    
    /// Initialize GStreamer (call once at startup)
    pub fn init_gstreamer() -> Result<()> {
        gst::init().context("Failed to initialize GStreamer")?;
        
        // Log GStreamer version
        let (major, minor, micro, nano) = gst::version();
        info!(
            major,
            minor,
            micro,
            nano,
            "GStreamer initialized"
        );
        
        Ok(())
    }
    
    /// Build the GStreamer pipeline
    pub fn build(&mut self) -> Result<()> {
        let camera_id = &self.config.camera_id;
        
        debug!(camera_id = %camera_id, "Building GStreamer pipeline");
        
        // Sanitize RTSP URL for logging (remove credentials)
        let sanitized_url = Self::sanitize_rtsp_url(&self.config.rtsp_url);
        debug!(url = %sanitized_url, "RTSP URL (sanitized)");
        
        // Build pipeline string
        let pipeline_str = self.build_pipeline_string();
        
        // Parse and create pipeline
        let pipeline = gst::parse::launch(&pipeline_str)
            .map_err(|e| DetectorError::GStreamerError(format!(
                "Failed to create pipeline for {}: {}", camera_id, e
            )))?
            .dynamic_cast::<gst::Pipeline>()
            .map_err(|_| DetectorError::GStreamerError(
                "Failed to cast to Pipeline".to_string()
            ))?;
        
        // Get appsink element
        let appsink = pipeline
            .by_name("sink")
            .ok_or_else(|| DetectorError::GStreamerError(
                "Failed to get appsink element".to_string()
            ))?
            .dynamic_cast::<gst_app::AppSink>()
            .map_err(|_| DetectorError::GStreamerError(
                "Failed to cast to AppSink".to_string()
            ))?;
        
        // Configure appsink
        self.configure_appsink(&appsink);
        
        // Set up bus message handling
        self.setup_bus_handler(&pipeline)?;
        
        self.pipeline = Some(pipeline);
        self.appsink = Some(appsink);
        
        info!(camera_id = %camera_id, "Pipeline built successfully");
        
        Ok(())
    }
    
    /// Build the pipeline string for GStreamer
    fn build_pipeline_string(&self) -> String {
        let imgsz = self.config.imgsz;
        
        // Optimized pipeline for low latency RTSP streaming
        // - rtspsrc: Handle reconnection internally
        // - rtph264depay: H.264 specific, adjust for other codecs
        // - avdec_h264: Hardware accelerated if available
        // - videoconvert: Convert to RGB for inference
        // - videoscale: Resize to target size
        // - appsink: Access frames from Rust
        format!(
            r#"
            rtspsrc location="{rtsp_url}" latency=100 buffer-mode=auto 
                protocols=tcp drop-on-latency=true do-retransmission=false
                name=src
            ! rtph264depay
            ! h264parse
            ! avdec_h264 max-threads=2
            ! videoconvert
            ! videoscale
            ! video/x-raw,format=RGB,width={width},height={height}
            ! appsink name=sink emit-signals=true sync=false 
                max-buffers=1 drop=true
            "#,
            rtsp_url = self.config.rtsp_url,
            width = imgsz,
            height = imgsz,
        )
        .trim()
        .replace('\n', " ")
        .replace("  ", " ")
    }
    
    /// Configure appsink for optimal performance
    fn configure_appsink(&self, appsink: &gst_app::AppSink) {
        // Set caps for RGB format at target size
        let caps = gst::Caps::builder("video/x-raw")
            .field("format", "RGB")
            .field("width", self.config.imgsz as i32)
            .field("height", self.config.imgsz as i32)
            .build();
        
        appsink.set_caps(Some(&caps));
        
        // Enable dropping old buffers when queue is full
        appsink.set_drop(true);
        appsink.set_max_buffers(1);
        appsink.set_sync(false);
        // Note: set_emit_signals is deprecated/removed in newer GStreamer versions
        // Signals are emitted automatically when needed
    }
    
    /// Set up bus message handler for pipeline events
    fn setup_bus_handler(&self, pipeline: &gst::Pipeline) -> Result<()> {
        let camera_id = self.config.camera_id.clone();
        
        let bus = pipeline
            .bus()
            .ok_or_else(|| DetectorError::GStreamerError(
                "Failed to get pipeline bus".to_string()
            ))?;
        
        // Add watch for bus messages
        let _watch = bus.add_watch(move |_, msg| {
            use gst::MessageView;
            
            match msg.view() {
                MessageView::Error(err) => {
                    error!(
                        camera_id = %camera_id,
                        error = %err.error(),
                        debug = ?err.debug(),
                        "Pipeline error"
                    );
                }
                MessageView::Warning(warn) => {
                    warn!(
                        camera_id = %camera_id,
                        warning = %warn.error(),
                        "Pipeline warning"
                    );
                }
                MessageView::StateChanged(state) => {
                    if state.src().map(|s| s.name().as_str() == "pipeline").unwrap_or(false) {
                        debug!(
                            camera_id = %camera_id,
                            old = ?state.old(),
                            new = ?state.current(),
                            "Pipeline state changed"
                        );
                    }
                }
                MessageView::Eos(_) => {
                    warn!(camera_id = %camera_id, "End of stream");
                }
                MessageView::Latency(_) => {
                    debug!(camera_id = %camera_id, "Latency update");
                }
                _ => {}
            }
            
            gst::glib::ControlFlow::Continue
        })?;
        
        Ok(())
    }
    
    /// Start the pipeline
    pub fn start(&mut self) -> Result<()> {
        if let Some(ref pipeline) = self.pipeline {
            pipeline
                .set_state(gst::State::Playing)
                .map_err(|e| DetectorError::GStreamerError(format!(
                    "Failed to start pipeline: {:?}", e
                )))?;
            
            self.is_running = true;
            info!(camera_id = %self.config.camera_id, "Pipeline started");
        }
        
        Ok(())
    }
    
    /// Stop the pipeline
    pub fn stop(&mut self) -> Result<()> {
        if let Some(ref pipeline) = self.pipeline {
            pipeline
                .set_state(gst::State::Null)
                .map_err(|e| DetectorError::GStreamerError(format!(
                    "Failed to stop pipeline: {:?}", e
                )))?;
            
            self.is_running = false;
            info!(camera_id = %self.config.camera_id, "Pipeline stopped");
        }
        
        Ok(())
    }
    
    /// Pull a frame from the appsink (blocking)
    pub fn pull_frame(&self) -> Result<Option<Frame>> {
        let appsink = self.appsink.as_ref()
            .ok_or_else(|| DetectorError::GStreamerError(
                "Pipeline not initialized".to_string()
            ))?;
        
        // Try to pull sample with timeout
        match appsink.try_pull_sample(gst::ClockTime::from_mseconds(100)) {
            Some(sample) => {
                let buffer = sample.buffer()
                    .ok_or_else(|| DetectorError::GStreamerError(
                        "No buffer in sample".to_string()
                    ))?;
                
                let map = buffer.map_readable()
                    .map_err(|e| DetectorError::GStreamerError(format!(
                        "Failed to map buffer: {:?}", e
                    )))?;
                
                let caps = sample.caps()
                    .ok_or_else(|| DetectorError::GStreamerError(
                        "No caps in sample".to_string()
                    ))?;
                
                let info = gstreamer_video::VideoInfo::from_caps(caps)
                    .map_err(|e| DetectorError::GStreamerError(format!(
                        "Failed to get video info: {:?}", e
                    )))?;
                
                let frame = Frame {
                    data: map.to_vec(),
                    width: info.width() as u32,
                    height: info.height() as u32,
                    timestamp: buffer.pts().map(|pts| pts.mseconds()).unwrap_or(0),
                };
                
                Ok(Some(frame))
            }
            None => Ok(None),
        }
    }
    
    /// Check if pipeline is running
    pub fn is_running(&self) -> bool {
        self.is_running
    }
    
    /// Get current pipeline state
    pub fn state(&self) -> Option<gst::State> {
        self.pipeline.as_ref().map(|p| {
            let (_, current, _) = p.state(gst::ClockTime::ZERO);
            current
        })
    }
    
    /// Sanitize RTSP URL for logging (remove credentials)
    fn sanitize_rtsp_url(url: &str) -> String {
        // Pattern: rtsp://user:pass@host:port/path
        if let Some(at_pos) = url.find('@') {
            if let Some(proto_end) = url.find("://") {
                let proto = &url[..proto_end + 3];
                let rest = &url[at_pos + 1..];
                return format!("{}****:****@{}", proto, rest);
            }
        }
        url.to_string()
    }
}

impl Drop for CameraPipeline {
    fn drop(&mut self) {
        if let Err(e) = self.stop() {
            error!(
                camera_id = %self.config.camera_id,
                error = %e,
                "Failed to stop pipeline on drop"
            );
        }
    }
}

/// A single video frame
#[derive(Debug, Clone)]
pub struct Frame {
    /// RGB pixel data (row-major, 3 bytes per pixel)
    pub data: Vec<u8>,
    
    /// Frame width in pixels
    pub width: u32,
    
    /// Frame height in pixels
    pub height: u32,
    
    /// Presentation timestamp in milliseconds
    pub timestamp: u64,
}

impl Frame {
    /// Get frame size in bytes
    pub fn size(&self) -> usize {
        self.data.len()
    }
    
    /// Check if frame has valid dimensions
    pub fn is_valid(&self) -> bool {
        !self.data.is_empty()
            && self.width > 0
            && self.height > 0
            && self.data.len() == (self.width * self.height * 3) as usize
    }
    
    /// Convert to image::RgbImage
    pub fn to_image(&self) -> Option<image::RgbImage> {
        image::RgbImage::from_raw(self.width, self.height, self.data.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_rtsp_url() {
        let url = "rtsp://admin:password123@192.168.1.100:554/stream1";
        let sanitized = CameraPipeline::sanitize_rtsp_url(url);
        assert!(!sanitized.contains("admin"));
        assert!(!sanitized.contains("password123"));
        assert!(sanitized.contains("192.168.1.100"));
    }

    #[test]
    fn test_sanitize_rtsp_url_no_auth() {
        let url = "rtsp://192.168.1.100:554/stream1";
        let sanitized = CameraPipeline::sanitize_rtsp_url(url);
        assert_eq!(sanitized, url);
    }

    #[test]
    fn test_frame_validity() {
        let frame = Frame {
            data: vec![0u8; 640 * 640 * 3],
            width: 640,
            height: 640,
            timestamp: 0,
        };
        assert!(frame.is_valid());

        let invalid_frame = Frame {
            data: vec![0u8; 100],
            width: 640,
            height: 640,
            timestamp: 0,
        };
        assert!(!invalid_frame.is_valid());
    }
}
