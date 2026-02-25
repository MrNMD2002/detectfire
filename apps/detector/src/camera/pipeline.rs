//! GStreamer pipeline for RTSP ingestion (MBFS-Stream push model)
//!
//! Frame data is stored as `Arc<Vec<u8>>` (taken from MBFS-Stream `PluginFrame.data`)
//! so that multiple subscribers receive zero-copy references to the same pixel buffer.
//!
//! Uses set_callbacks on appsink (push model) instead of try_pull_sample (pull model).
//! Frames are broadcast via tokio::sync::broadcast so multiple consumers can subscribe:
//!   - inference worker (via CameraWorker::subscribe)
//!   - live MJPEG stream server (via CameraManager::subscribe_to_camera)
//!
//! Architecture (from MBFS-Stream):
//!   rtspsrc (dynamic pads)
//!     -> [pad_added: depay + parse + decoder]
//!     -> queue -> videoconvert -> videoscale -> capsfilter(RGB) -> appsink (set_callbacks)
//!                                                                        |
//!                                                              broadcast::Sender<Arc<Frame>>

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::config::CameraConfig;
use crate::error::DetectorError;

/// GStreamer pipeline for a single camera
pub struct CameraPipeline {
    /// Camera configuration
    config: CameraConfig,

    /// GStreamer pipeline
    pipeline: Option<gst::Pipeline>,

    /// Pipeline state
    is_running: bool,

    /// Guard: prevents double-linking decoder chain in connect_pad_added
    decoder_linked: Arc<AtomicBool>,

    /// Broadcast sender — shared with CameraWorker for lifetime stability across reconnects
    frame_tx: broadcast::Sender<Arc<Frame>>,
}

impl CameraPipeline {
    /// Create a new camera pipeline.
    /// `frame_tx` is owned by the CameraWorker and outlives individual pipeline instances.
    pub fn new(config: CameraConfig, frame_tx: broadcast::Sender<Arc<Frame>>) -> Self {
        Self {
            config,
            pipeline: None,
            is_running: false,
            decoder_linked: Arc::new(AtomicBool::new(false)),
            frame_tx,
        }
    }

    /// Initialize GStreamer (call once at startup)
    pub fn init_gstreamer() -> Result<()> {
        gst::init().context("Failed to initialize GStreamer")?;

        let (major, minor, micro, nano) = gst::version();
        info!(major, minor, micro, nano, "GStreamer initialized");

        Ok(())
    }

    /// Build the element-based pipeline with dynamic codec detection.
    ///
    /// Architecture (MBFS-Stream PluginStream):
    ///   rtspsrc (dynamic pads)
    ///     -> [pad_added: depay + parse + decoder]
    ///     -> queue(leaky=downstream) -> videoconvert -> videoscale
    ///     -> capsfilter(RGB, imgsz×imgsz) -> appsink(set_callbacks → broadcast)
    pub fn build(&mut self) -> Result<()> {
        let camera_id = &self.config.camera_id;
        let imgsz = self.config.imgsz as i32;

        debug!(camera_id = %camera_id, "Building GStreamer pipeline (MBFS push model)");
        debug!(url = %Self::sanitize_rtsp_url(&self.config.rtsp_url), "RTSP source");

        // Reset decoder-linked guard on each rebuild (reconnect)
        self.decoder_linked.store(false, Ordering::SeqCst);

        let pipeline = gst::Pipeline::new();

        // ── RTSP source (MBFS-Stream optimizations) ──────────────────────────────
        let rtspsrc = gst::ElementFactory::make("rtspsrc")
            .property("location", &self.config.rtsp_url)
            .property("latency", 50u32)           // 50ms jitter buffer (MBFS PluginStreamConfig default)
            .property_from_str("protocols", "tcp") // Force TCP: avoids UDP loss
            .property_from_str("buffer-mode", "slave") // Lowest latency, slave to server clock
            .property("do-rtcp", true)
            .property("drop-on-latency", true)     // Drop late packets immediately
            .property("do-retransmission", false)  // No RTCP retransmission (MBFS-Stream)
            .property("timeout", 10_000_000u64)    // 10s in microseconds
            .property("tcp-timeout", 10_000_000u64)
            .property("retry", 3u32)
            .build()
            .map_err(|e| DetectorError::GStreamerError(format!("Failed to create rtspsrc: {}", e)))?;

        // ── Post-decode chain (static) ────────────────────────────────────────────
        // queue: 2 buffers (MBFS-Stream), leaky=downstream drops oldest when full
        let queue = gst::ElementFactory::make("queue")
            .property("max-size-buffers", 2u32)
            .property_from_str("leaky", "downstream")
            .build()
            .map_err(|e| DetectorError::GStreamerError(format!("Failed to create queue: {}", e)))?;

        let videoconvert = gst::ElementFactory::make("videoconvert")
            .build()
            .map_err(|e| DetectorError::GStreamerError(format!("Failed to create videoconvert: {}", e)))?;

        let videoscale = gst::ElementFactory::make("videoscale")
            .build()
            .map_err(|e| DetectorError::GStreamerError(format!("Failed to create videoscale: {}", e)))?;

        // capsfilter: RGB output at model input size (same as MBFS-Stream create_rgb_caps)
        let capsfilter = gst::ElementFactory::make("capsfilter")
            .property(
                "caps",
                gst::Caps::builder("video/x-raw")
                    .field("format", "RGB")
                    .field("width", imgsz)
                    .field("height", imgsz)
                    .build(),
            )
            .build()
            .map_err(|e| DetectorError::GStreamerError(format!("Failed to create capsfilter: {}", e)))?;

        // appsink: max-buffers=2, drop=true, sync=false (MBFS-Stream create_appsink)
        // emit-signals=false: we use set_callbacks (lower overhead than GLib signals)
        let appsink_elem = gst::ElementFactory::make("appsink")
            .name("sink")
            .property("max-buffers", 2u32)
            .property("drop", true)
            .property("sync", false)
            .build()
            .map_err(|e| DetectorError::GStreamerError(format!("Failed to create appsink: {}", e)))?;

        // Add all static elements
        pipeline
            .add_many([&rtspsrc, &queue, &videoconvert, &videoscale, &capsfilter, &appsink_elem])
            .map_err(|e| DetectorError::GStreamerError(format!("Failed to add elements: {}", e)))?;

        // Link the static chain (decoder chain is linked dynamically in pad_added)
        gst::Element::link_many([&queue, &videoconvert, &videoscale, &capsfilter, &appsink_elem])
            .map_err(|e| DetectorError::GStreamerError(format!("Failed to link static chain: {}", e)))?;

        // ── Dynamic pad handler (MBFS-Stream pattern) ─────────────────────────────
        let pipeline_weak = pipeline.downgrade();
        let queue_weak = queue.downgrade();
        let camera_id_clone = camera_id.clone();
        let codec_hint = self.config.codec.to_lowercase();
        let decoder_linked = Arc::clone(&self.decoder_linked);

        rtspsrc.connect_pad_added(move |_src, src_pad| {
            // 1. Determine codec from pad caps
            let caps = src_pad
                .current_caps()
                .or_else(|| Some(src_pad.query_caps(None)));

            let Some(caps) = caps else {
                debug!(camera_id = %camera_id_clone, "Pad has no caps, skipping");
                return;
            };
            let Some(structure) = caps.structure(0) else {
                debug!(camera_id = %camera_id_clone, "Caps has no structure, skipping");
                return;
            };

            let media_type = structure.name().to_string();
            let encoding_name = structure.get::<String>("encoding-name").ok();

            debug!(camera_id = %camera_id_clone, encoding = ?encoding_name, "rtspsrc pad added");

            if !media_type.starts_with("application/x-rtp") {
                debug!(camera_id = %camera_id_clone, media_type = %media_type, "Non-RTP pad, skipping");
                return;
            }

            let codec: String = match encoding_name.as_deref() {
                Some("H264") => "h264".to_string(),
                Some("H265") | Some("HEVC") => "h265".to_string(),
                Some(other) => {
                    debug!(camera_id = %camera_id_clone, encoding = other, "Unknown encoding, skipping pad");
                    return;
                }
                None => {
                    let is_video = {
                        let c2 = src_pad.current_caps().or_else(|| Some(src_pad.query_caps(None)));
                        c2.as_ref()
                            .and_then(|c| c.structure(0))
                            .and_then(|s| s.get::<String>("media").ok())
                            .map(|m| m == "video")
                            .unwrap_or(false)
                    };
                    if is_video {
                        warn!(
                            camera_id = %camera_id_clone,
                            "encoding-name absent on video pad; using configured codec: {}",
                            codec_hint
                        );
                        codec_hint.clone()
                    } else {
                        debug!(camera_id = %camera_id_clone, "Non-video RTP pad, skipping");
                        return;
                    }
                }
            };

            // 2. Guard: only link once
            if decoder_linked.swap(true, Ordering::SeqCst) {
                debug!(camera_id = %camera_id_clone, "Decoder already linked, skipping");
                return;
            }

            let Some(pipeline) = pipeline_weak.upgrade() else { return };
            let Some(queue) = queue_weak.upgrade() else { return };

            // 3. Build decoder chain: [depay, parse, decoder]
            let elements = match Self::create_decoder_chain(&codec) {
                Ok(e) => e,
                Err(e) => {
                    error!(camera_id = %camera_id_clone, error = %e, "Failed to create decoder chain");
                    return;
                }
            };

            // 4. Add decoder elements to pipeline
            for element in &elements {
                if let Err(e) = pipeline.add(element) {
                    error!(camera_id = %camera_id_clone, error = %e, "Failed to add decoder element");
                    return;
                }
            }

            // 5. Link: [0] → [1] → [2]
            for window in elements.windows(2) {
                if let Err(e) = window[0].link(&window[1]) {
                    error!(camera_id = %camera_id_clone, error = %e, "Failed to link decoder elements");
                    return;
                }
            }

            // 6. Link last decoder → queue
            if let Some(last) = elements.last() {
                if let Err(e) = last.link(&queue) {
                    error!(camera_id = %camera_id_clone, error = %e, "Failed to link decoder to queue");
                    return;
                }
            }

            // 7. Sync decoder state with running pipeline
            for element in &elements {
                let _ = element.sync_state_with_parent();
            }

            // 8. Link rtspsrc src pad → depay sink pad
            if let Some(first) = elements.first() {
                if let Some(sink_pad) = first.static_pad("sink") {
                    match src_pad.link(&sink_pad) {
                        Ok(_) => info!(
                            camera_id = %camera_id_clone,
                            codec = %codec,
                            "Decoder chain connected to rtspsrc"
                        ),
                        Err(e) => error!(
                            camera_id = %camera_id_clone,
                            error = ?e,
                            "Failed to link rtspsrc pad to decoder"
                        ),
                    }
                }
            }
        });

        // ── Cast appsink and register push callbacks (MBFS-Stream set_callbacks) ──
        let appsink = appsink_elem
            .dynamic_cast::<gst_app::AppSink>()
            .map_err(|_| DetectorError::GStreamerError("Failed to cast to AppSink".to_string()))?;

        // Belt-and-suspenders caps on appsink itself
        let caps = gst::Caps::builder("video/x-raw")
            .field("format", "RGB")
            .field("width", imgsz)
            .field("height", imgsz)
            .build();
        appsink.set_caps(Some(&caps));

        // Push model: callback fires on GStreamer's thread, sends Arc<Frame> to broadcast channel.
        // All subscribers (inference worker, MJPEG server) receive the same Arc without copying pixels.
        let frame_tx = self.frame_tx.clone();
        let out_size = (imgsz as u32, imgsz as u32);

        appsink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Error)?;

                    if let Some(buffer) = sample.buffer() {
                        let map = match buffer.map_readable() {
                            Ok(m) => m,
                            Err(_) => return Ok(gst::FlowSuccess::Ok),
                        };
                        let data = map.as_slice();
                        let expected = (out_size.0 * out_size.1 * 3) as usize;

                        if data.len() >= expected {
                            // Arc<Vec<u8>>: multiple subscribers share the same pixel buffer
                            // without copying (MBFS-Stream PluginFrame.data pattern)
                            let frame = Arc::new(Frame {
                                data: Arc::new(data[..expected].to_vec()),
                                width: out_size.0,
                                height: out_size.1,
                                timestamp: buffer.pts().map(|p| p.mseconds()).unwrap_or(0),
                            });
                            // Ignore SendError when no receivers (normal at startup)
                            let _ = frame_tx.send(frame);
                        }
                    }

                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        self.setup_bus_handler(&pipeline)?;

        self.pipeline = Some(pipeline);

        info!(camera_id = %camera_id, "Pipeline built (push model)");
        Ok(())
    }

    /// Create complete decoder chain for a codec: [depay, parse, decoder]
    fn create_decoder_chain(codec: &str) -> Result<Vec<gst::Element>, String> {
        let (depay_name, parse_name) = match codec {
            "h265" | "hevc" => ("rtph265depay", "h265parse"),
            _ => ("rtph264depay", "h264parse"),
        };

        let depay = gst::ElementFactory::make(depay_name)
            .build()
            .map_err(|e| format!("Failed to create {}: {}", depay_name, e))?;

        let parse = gst::ElementFactory::make(parse_name)
            .build()
            .map_err(|e| format!("Failed to create {}: {}", parse_name, e))?;

        let decoder = Self::create_best_decoder(codec)?;

        Ok(vec![depay, parse, decoder])
    }

    /// Find and create the best available decoder for a codec.
    ///
    /// Priority order mirrors MBFS-Stream decoder.rs h264/h265_decoders():
    ///   Linux NVIDIA (nvdec) → VAAPI (Intel/AMD) → QuickSync → FFmpeg software
    fn create_best_decoder(codec: &str) -> Result<gst::Element, String> {
        // Priority order from MBFS-Stream decoder.rs:
        //   NVIDIA nvdec → VAAPI (Intel/AMD) → Intel QuickSync → FFmpeg → OpenH264 (last resort)
        let candidates: &[&str] = match codec {
            "h265" | "hevc" => &["nvh265dec", "vaapih265dec", "qsvh265dec", "avdec_h265"],
            _ => &["nvh264dec", "vaapih264dec", "qsvh264dec", "avdec_h264", "openh264dec"],
        };

        for &name in candidates {
            let ok = gst::ElementFactory::find(name)
                .and_then(|f| f.create().build().ok())
                .is_some();
            if !ok {
                debug!(decoder = name, "Not available, skipping");
                continue;
            }

            let element = gst::ElementFactory::make(name)
                .build()
                .map_err(|e| format!("Failed to create {}: {}", name, e))?;

            // NVIDIA: extra settings from MBFS-Stream decoder.rs
            if name.starts_with("nv") {
                if element.has_property("num-output-surfaces", None) {
                    element.set_property("num-output-surfaces", 8u32);
                }
                if element.has_property("max-display-delay", None) {
                    element.set_property("max-display-delay", 0i32);
                }
            }

            // FFmpeg software: multi-thread for lower decode latency
            if name.starts_with("avdec") {
                if element.has_property("max-threads", None) {
                    element.set_property("max-threads", 4i32);
                }
            }

            info!(decoder = name, codec, "Selected decoder");
            return Ok(element);
        }

        Err(format!("No decoder available for codec '{}'", codec))
    }

    /// Set up bus message handler for pipeline events
    fn setup_bus_handler(&self, pipeline: &gst::Pipeline) -> Result<()> {
        let camera_id = self.config.camera_id.clone();

        let bus = pipeline
            .bus()
            .ok_or_else(|| DetectorError::GStreamerError("Failed to get pipeline bus".to_string()))?;

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
                    if state
                        .src()
                        .map(|s| s.name().as_str() == "pipeline")
                        .unwrap_or(false)
                    {
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
                .map_err(|e| DetectorError::GStreamerError(format!("Failed to start pipeline: {:?}", e)))?;

            self.is_running = true;
            info!(camera_id = %self.config.camera_id, "Pipeline started (push model)");
        }
        Ok(())
    }

    /// Stop the pipeline
    pub fn stop(&mut self) -> Result<()> {
        if let Some(ref pipeline) = self.pipeline {
            pipeline
                .set_state(gst::State::Null)
                .map_err(|e| DetectorError::GStreamerError(format!("Failed to stop pipeline: {:?}", e)))?;

            self.is_running = false;
            info!(camera_id = %self.config.camera_id, "Pipeline stopped");
        }
        Ok(())
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

/// A single video frame (RGB, row-major, 3 bytes per pixel)
///
/// Pixel data is held in `Arc<Vec<u8>>` so that cloning a `Frame` (e.g. via
/// `broadcast::Receiver`) does **not** copy the pixel buffer — identical to
/// MBFS-Stream's `PluginFrame.data: Arc<Vec<u8>>`.
#[derive(Debug, Clone)]
pub struct Frame {
    /// RGB pixel data (shared, zero-copy on clone)
    pub data: Arc<Vec<u8>>,

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
    ///
    /// Clones the underlying `Vec<u8>` once to satisfy `image::RgbImage::from_raw`'s
    /// ownership requirement. The `Arc` wrapper avoids any extra allocation on the
    /// broadcast path; only the single caller that needs an `RgbImage` pays the copy.
    pub fn to_image(&self) -> Option<image::RgbImage> {
        image::RgbImage::from_raw(self.width, self.height, self.data.as_ref().clone())
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
            data: Arc::new(vec![0u8; 640 * 640 * 3]),
            width: 640,
            height: 640,
            timestamp: 0,
        };
        assert!(frame.is_valid());

        let invalid_frame = Frame {
            data: Arc::new(vec![0u8; 100]),
            width: 640,
            height: 640,
            timestamp: 0,
        };
        assert!(!invalid_frame.is_valid());
    }

    #[test]
    fn test_frame_clone_is_zero_copy() {
        let data = Arc::new(vec![0u8; 640 * 640 * 3]);
        let frame = Arc::new(Frame {
            data: Arc::clone(&data),
            width: 640,
            height: 640,
            timestamp: 0,
        });
        // Cloning Frame only bumps Arc refcounts — no pixel data copied
        let cloned = frame.clone();
        assert!(Arc::ptr_eq(&frame.data, &cloned.data));
    }
}
