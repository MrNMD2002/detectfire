//! Event publisher
//!
//! Publishes events to API service and handles snapshot saving.
//! Snapshots are annotated with colored bounding boxes and confidence labels
//! before being saved to disk and forwarded to Telegram.

use std::path::PathBuf;
use std::sync::Arc;
use chrono::Utc;
use image::codecs::jpeg::JpegEncoder;
use ab_glyph::{FontArc, PxScale};
use image::Rgb;
use imageproc::drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use parking_lot::RwLock;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::camera::Frame;
use crate::config::{GlobalConfig, SnapshotConfig};
use crate::decision::{DecisionEvent, DecisionEventType};
use crate::error::DetectorResult;
use crate::inference::{Detection, DetectionClass};

use super::models::{EventMetadata, EventType, FireEvent};

/// Event publisher for broadcasting events
#[derive(Clone)]
pub struct EventPublisher {
    inner: Arc<EventPublisherInner>,
}

struct EventPublisherInner {
    /// Broadcast channel for events
    tx: broadcast::Sender<FireEvent>,

    /// Snapshot configuration
    snapshot_config: SnapshotConfig,

    /// Base path for snapshots
    snapshot_base_path: PathBuf,

    /// Event counter
    event_count: RwLock<u64>,

    /// TrueType font for bounding box labels.
    /// None if no system font was found — boxes are still drawn, just without text.
    font: Option<FontArc>,
}

/// Default capacity for the event broadcast channel.
/// Increase if downstream gRPC consumers cannot keep up.
const DEFAULT_BROADCAST_CAPACITY: usize = 1_024;

impl EventPublisher {
    /// Create a new event publisher
    pub fn new(config: &GlobalConfig) -> DetectorResult<Self> {
        let capacity = std::env::var("DETECTOR__EVENT_BROADCAST_CAPACITY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(DEFAULT_BROADCAST_CAPACITY);

        let (tx, _) = broadcast::channel(capacity);

        let snapshot_base_path = PathBuf::from(&config.storage.snapshots.path);

        // Create snapshot directory if enabled – propagate errors so misconfiguration
        // is caught at startup rather than silently losing snapshots at runtime.
        if config.storage.snapshots.enabled {
            std::fs::create_dir_all(&snapshot_base_path).map_err(|e| {
                crate::error::DetectorError::SnapshotError(format!(
                    "Failed to create snapshot directory '{}': {}",
                    snapshot_base_path.display(),
                    e
                ))
            })?;
        }

        // Load font for bounding box labels (best-effort: warn and continue if not found)
        let font = load_system_font();

        Ok(Self {
            inner: Arc::new(EventPublisherInner {
                tx,
                snapshot_config: config.storage.snapshots.clone(),
                snapshot_base_path,
                event_count: RwLock::new(0),
                font,
            }),
        })
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<FireEvent> {
        self.inner.tx.subscribe()
    }

    /// Publish a detection event
    pub async fn publish(&self, decision: DecisionEvent, frame: &Frame) -> DetectorResult<()> {
        let event_type = match decision.event_type {
            DecisionEventType::Fire => EventType::Fire,
            DecisionEventType::Smoke => EventType::Smoke,
        };

        // Save snapshot with bounding boxes BEFORE moving detections into FireEvent.
        // decision.detections is borrowed here, then moved into FireEvent::new() below.
        let snapshot_result = if self.inner.snapshot_config.enabled {
            match self
                .save_snapshot(frame, &decision.camera_id, &decision.detections)
                .await
            {
                Ok(result) => Some(result),
                Err(e) => {
                    warn!(
                        camera_id = %decision.camera_id,
                        error = %e,
                        "Failed to save snapshot"
                    );
                    None
                }
            }
        } else {
            None
        };

        // Create base event (consumes decision.detections)
        let mut event = FireEvent::new(
            event_type,
            &decision.camera_id,
            &decision.site_id,
            decision.confidence,
            decision.detections,
        );

        // Apply snapshot result
        if let Some((path, data)) = snapshot_result {
            event.snapshot_path = Some(path);
            event.snapshot_data = Some(data);
        }

        // Add metadata
        event.metadata = EventMetadata {
            frame_width: Some(frame.width),
            frame_height: Some(frame.height),
            ..Default::default()
        };

        // Broadcast event
        self.broadcast_event(event);

        Ok(())
    }

    /// Publish stream down event
    pub async fn publish_stream_down(&self, camera_id: &str, error: &str) -> DetectorResult<()> {
        let event = FireEvent::stream_down(camera_id, "", error);
        self.broadcast_event(event);
        Ok(())
    }

    /// Publish stream up event
    pub async fn publish_stream_up(&self, camera_id: &str) -> DetectorResult<()> {
        let event = FireEvent::stream_up(camera_id, "");
        self.broadcast_event(event);
        Ok(())
    }

    /// Save snapshot annotated with bounding boxes, return (relative_path, jpeg_bytes).
    async fn save_snapshot(
        &self,
        frame: &Frame,
        camera_id: &str,
        detections: &[Detection],
    ) -> DetectorResult<(String, Vec<u8>)> {
        // Convert frame to image
        let mut img = frame
            .to_image()
            .ok_or_else(|| {
                crate::error::DetectorError::SnapshotError(
                    "Failed to convert frame to image".to_string(),
                )
            })?;

        // Annotate with bounding boxes and confidence labels
        draw_bounding_boxes(&mut img, detections, self.inner.font.as_ref());

        // Encode as JPEG
        let mut jpeg_bytes = Vec::new();
        {
            let mut encoder =
                JpegEncoder::new_with_quality(&mut jpeg_bytes, self.inner.snapshot_config.quality);

            encoder
                .encode_image(&img)
                .map_err(|e| crate::error::DetectorError::SnapshotError(e.to_string()))?;
        }

        // Generate filename
        let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%S%.3f");
        let filename = format!("{}_{}.jpg", camera_id, timestamp);

        // Create camera subdirectory
        let camera_dir = self.inner.snapshot_base_path.join(camera_id);
        if let Err(e) = std::fs::create_dir_all(&camera_dir) {
            warn!(
                camera_id = %camera_id,
                path = %camera_dir.display(),
                error = %e,
                "Failed to create camera snapshot directory"
            );
        }

        // Save file
        let filepath = camera_dir.join(&filename);
        tokio::fs::write(&filepath, &jpeg_bytes)
            .await
            .map_err(|e| crate::error::DetectorError::SnapshotError(e.to_string()))?;

        // Return relative path for database
        let relative_path = format!("{}/{}", camera_id, filename);

        debug!(
            camera_id = %camera_id,
            path = %relative_path,
            size_kb = jpeg_bytes.len() / 1024,
            boxes = detections.len(),
            "Snapshot saved with bounding boxes"
        );

        Ok((relative_path, jpeg_bytes))
    }

    /// Broadcast event to all subscribers
    fn broadcast_event(&self, event: FireEvent) {
        // Increment counter
        *self.inner.event_count.write() += 1;

        // Log event
        info!(
            event_id = %event.event_id,
            event_type = %event.event_type,
            camera_id = %event.camera_id,
            site_id = %event.site_id,
            confidence = event.confidence,
            detections = event.detections.len(),
            "Publishing event"
        );

        // Broadcast
        if self.inner.tx.receiver_count() > 0 {
            if let Err(e) = self.inner.tx.send(event) {
                error!(error = %e, "Failed to broadcast event");
            }
        } else {
            debug!("No subscribers, event not broadcast");
        }
    }

    /// Get total event count
    pub fn event_count(&self) -> u64 {
        *self.inner.event_count.read()
    }

    /// Get subscriber count
    pub fn subscriber_count(&self) -> usize {
        self.inner.tx.receiver_count()
    }
}

// ── Bounding box drawing ──────────────────────────────────────────────────────

/// Box color per detection class:
///   Fire  → red    (255, 50,  50)
///   Smoke → orange (255, 165,  0)
///   Other → yellow (255, 220,  0)
#[inline]
fn class_color(class: DetectionClass) -> Rgb<u8> {
    match class {
        DetectionClass::Fire => Rgb([255, 50, 50]),
        DetectionClass::Smoke => Rgb([255, 165, 0]),
        DetectionClass::Other => Rgb([255, 220, 0]),
    }
}

/// Draw colored bounding boxes and confidence labels on the image.
///
/// Each detection gets a 3-pixel wide colored border.  When a font is
/// available, a filled label tag (e.g. "fire 92%") is drawn above the box.
fn draw_bounding_boxes(
    img: &mut image::RgbImage,
    detections: &[Detection],
    font: Option<&FontArc>,
) {
    let (img_w, img_h) = img.dimensions();

    for det in detections {
        let color = class_color(det.class);

        // Denormalize normalized bbox → pixel coordinates
        let (bx, by, bw, bh) = det.bbox.to_pixels(img_w, img_h);
        let bx = bx.max(0) as u32;
        let by = by.max(0) as u32;
        let bw = (bw as u32).min(img_w.saturating_sub(bx)).max(2);
        let bh = (bh as u32).min(img_h.saturating_sub(by)).max(2);

        // Draw 3-pixel thick border by expanding the hollow rect outward
        for offset in 0i32..3 {
            let ox = (bx as i32 - offset).max(0);
            let oy = (by as i32 - offset).max(0);
            let rw = (bw + 2 * offset as u32).min(img_w.saturating_sub(ox as u32));
            let rh = (bh + 2 * offset as u32).min(img_h.saturating_sub(oy as u32));
            if rw >= 1 && rh >= 1 {
                draw_hollow_rect_mut(img, Rect::at(ox, oy).of_size(rw, rh), color);
            }
        }

        // Draw label tag if font is available
        if let Some(font) = font {
            let label = format!("{} {:.0}%", det.class.name(), det.confidence * 100.0);
            let scale = PxScale::from(16.0);

            // Approximate label dimensions (~9px/char wide, 18px tall at scale 16)
            let label_w = (label.len() as u32 * 9).min(img_w.saturating_sub(bx));
            let label_h = 18u32;

            // Place tag above the box; fall back to inside top edge if at image boundary
            let label_x = bx as i32;
            let label_y = if by >= label_h {
                (by - label_h) as i32
            } else {
                by as i32
            };

            // Filled colored background for readability
            if label_w >= 1 {
                draw_filled_rect_mut(
                    img,
                    Rect::at(label_x, label_y).of_size(label_w, label_h),
                    color,
                );
            }

            // Black text on the colored tag
            draw_text_mut(img, Rgb([0u8, 0u8, 0u8]), label_x, label_y, scale, font, &label);
        }
    }
}

/// Try to load a TrueType font from common system paths.
///
/// Checks several well-known locations on Linux (Docker) and Windows.
/// Returns `None` and emits a warning if no font is found — the system
/// will continue to draw boxes without text labels.
fn load_system_font() -> Option<FontArc> {
    const FONT_PATHS: &[&str] = &[
        // Debian/Ubuntu Docker images (install fonts-dejavu-core)
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        // Liberation fonts alternative
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        // Windows (development)
        "C:/Windows/Fonts/arialbd.ttf",
        "C:/Windows/Fonts/arial.ttf",
    ];

    for path in FONT_PATHS {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(font) = FontArc::try_from_vec(bytes) {
                info!(path = %path, "Bounding box font loaded");
                return Some(font);
            }
        }
    }

    warn!("No system font found - bounding box labels disabled (boxes still drawn)");
    None
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;

    fn make_test_config() -> GlobalConfig {
        GlobalConfig {
            server: ServerConfig {
                api: ApiServerConfig {
                    host: "0.0.0.0".to_string(),
                    port: 8080,
                    workers: 4,
                },
                detector: DetectorServerConfig {
                    host: "0.0.0.0".to_string(),
                    grpc_port: 50051,
                },
            },
            inference: InferenceConfig::default(),
            reconnect: ReconnectConfig::default(),
            telegram: TelegramConfig {
                enabled: false,
                bot_token: "".to_string(),
                default_chat_id: "".to_string(),
                rate_limit: TelegramRateLimit::default(),
                templates: TelegramTemplates::default(),
            },
            logging: LoggingConfig::default(),
            storage: StorageConfig {
                snapshots: SnapshotConfig {
                    enabled: false,
                    ..Default::default()
                },
                minio: MinioConfig::default(),
            },
            monitoring: MonitoringConfig::default(),
        }
    }

    #[tokio::test]
    async fn test_event_publisher_creation() {
        let config = make_test_config();
        let publisher = EventPublisher::new(&config).unwrap();

        assert_eq!(publisher.event_count(), 0);
        assert_eq!(publisher.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn test_event_publisher_subscribe() {
        let config = make_test_config();
        let publisher = EventPublisher::new(&config).unwrap();

        let _rx = publisher.subscribe();
        assert_eq!(publisher.subscriber_count(), 1);
    }
}
