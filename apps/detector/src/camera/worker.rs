//! Camera worker - per-camera processing loop (MBFS-Stream push model)
//!
//! Each camera has its own worker that:
//! - Owns a broadcast::Sender<Arc<Frame>> (survives across reconnects)
//! - Manages the pipeline lifecycle
//! - Subscribes to the broadcast for inference sampling
//! - Exposes subscribe() so StreamServer can also tap the same frames

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, broadcast};
use tracing::{debug, error, info, warn, Instrument};

use crate::config::{CameraConfig, ReconnectConfig};
use crate::camera::pipeline::{CameraPipeline, Frame};
use crate::camera::sampler::FrameSampler;
use crate::camera::status::{CameraStatus, StreamState};
use crate::error::DetectorResult;

/// Broadcast channel capacity (MBFS-Stream uses 16)
const FRAME_BROADCAST_CAPACITY: usize = 16;

/// Message sent from worker to manager
#[derive(Debug)]
pub enum WorkerMessage {
    /// Frame ready for inference
    Frame {
        camera_id: String,
        frame: Frame,
    },
    /// Stream state changed
    StateChanged {
        camera_id: String,
        state: StreamState,
        error: Option<String>,
    },
}

/// Camera worker handle
pub struct CameraWorker {
    /// Camera configuration
    config: CameraConfig,

    /// Reconnection configuration
    reconnect_config: ReconnectConfig,

    /// Channel to send messages to manager
    tx: mpsc::Sender<WorkerMessage>,

    /// Camera status (shared)
    status: Arc<CameraStatus>,

    /// Shutdown sender — dropping this aborts the worker task
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,

    /// Task handle
    task_handle: Option<tokio::task::JoinHandle<()>>,

    /// Frame broadcast sender — stable across reconnects, shared with StreamServer
    frame_tx: broadcast::Sender<Arc<Frame>>,
}

impl CameraWorker {
    /// Create a new camera worker
    pub fn new(
        config: CameraConfig,
        reconnect_config: ReconnectConfig,
        tx: mpsc::Sender<WorkerMessage>,
        status: Arc<CameraStatus>,
    ) -> Self {
        // Broadcast sender lives as long as the worker — outlives individual pipeline instances
        let (frame_tx, _) = broadcast::channel(FRAME_BROADCAST_CAPACITY);

        Self {
            config,
            reconnect_config,
            tx,
            status,
            shutdown_tx: None,
            task_handle: None,
            frame_tx,
        }
    }

    /// Subscribe to this camera's frame broadcast.
    /// Usable by inference worker internals and by the MJPEG stream server.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Frame>> {
        self.frame_tx.subscribe()
    }

    /// Start the worker
    pub fn start(&mut self) -> DetectorResult<()> {
        let (shutdown_tx, _shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        let config = self.config.clone();
        let reconnect_config = self.reconnect_config.clone();
        let tx = self.tx.clone();
        let status = self.status.clone();
        let frame_tx = self.frame_tx.clone();

        let camera_id = config.camera_id.clone();

        let handle = tokio::spawn(async move {
            let mut worker_loop = WorkerLoop::new(
                config,
                reconnect_config,
                tx,
                status,
                frame_tx,
            );

            if let Err(e) = worker_loop.run().await {
                error!(camera_id = %camera_id, error = %e, "Worker loop exited with error");
            }
        }.instrument(tracing::info_span!("camera_worker", camera_id = %self.config.camera_id)));

        self.task_handle = Some(handle);

        info!(camera_id = %self.config.camera_id, "Worker started");

        Ok(())
    }

    /// Stop the worker gracefully
    pub async fn stop(&mut self) {
        drop(self.shutdown_tx.take());

        if let Some(handle) = self.task_handle.take() {
            handle.abort();
            let _ = handle.await;
        }

        info!(camera_id = %self.config.camera_id, "Worker stopped");
    }

    /// Get camera status
    pub fn status(&self) -> &CameraStatus {
        &self.status
    }
}

/// Internal worker loop
struct WorkerLoop {
    config: CameraConfig,
    reconnect_config: ReconnectConfig,
    tx: mpsc::Sender<WorkerMessage>,
    status: Arc<CameraStatus>,
    pipeline: Option<CameraPipeline>,
    sampler: FrameSampler,

    /// Current reconnection delay
    current_delay: Duration,

    /// Consecutive errors count
    consecutive_errors: u32,

    /// Broadcast sender — passed to each new CameraPipeline on reconnect
    frame_tx: broadcast::Sender<Arc<Frame>>,
}

impl WorkerLoop {
    fn new(
        config: CameraConfig,
        reconnect_config: ReconnectConfig,
        tx: mpsc::Sender<WorkerMessage>,
        status: Arc<CameraStatus>,
        frame_tx: broadcast::Sender<Arc<Frame>>,
    ) -> Self {
        let sampler = FrameSampler::new(config.fps_sample);
        let initial_delay = Duration::from_millis(reconnect_config.initial_delay_ms);

        Self {
            config,
            reconnect_config,
            tx,
            status,
            pipeline: None,
            sampler,
            current_delay: initial_delay,
            consecutive_errors: 0,
            frame_tx,
        }
    }

    async fn run(&mut self) -> DetectorResult<()> {
        loop {
            if !self.config.enabled {
                self.update_state(StreamState::Disabled, None).await;
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            }

            match self.connect_and_stream().await {
                Ok(()) => {
                    break; // Normal exit (shutdown requested)
                }
                Err(e) => {
                    self.consecutive_errors += 1;

                    let error_msg = e.to_string();
                    error!(
                        camera_id = %self.config.camera_id,
                        error = %error_msg,
                        consecutive_errors = self.consecutive_errors,
                        "Stream error"
                    );

                    if self.consecutive_errors >= self.reconnect_config.max_retries {
                        self.update_state(StreamState::Failed, Some(error_msg)).await;
                        error!(
                            camera_id = %self.config.camera_id,
                            max_retries = self.reconnect_config.max_retries,
                            "Max retries exceeded, marking as failed"
                        );
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        self.reset_backoff();
                        continue;
                    }

                    self.update_state(StreamState::Reconnecting, Some(error_msg)).await;

                    info!(
                        camera_id = %self.config.camera_id,
                        delay_ms = self.current_delay.as_millis(),
                        attempt = self.consecutive_errors,
                        "Reconnecting after delay"
                    );

                    tokio::time::sleep(self.current_delay).await;
                    self.increase_backoff();
                }
            }
        }

        Ok(())
    }

    async fn connect_and_stream(&mut self) -> DetectorResult<()> {
        self.update_state(StreamState::Connecting, None).await;

        // Subscribe BEFORE starting pipeline so no early frames are missed
        // (broadcast channel capacity=16 buffers them)
        let rx = self.frame_tx.subscribe();

        // Build and start pipeline — passes the broadcast sender to appsink callbacks
        let mut pipeline = CameraPipeline::new(self.config.clone(), self.frame_tx.clone());
        pipeline.build()?;
        pipeline.start()?;

        self.pipeline = Some(pipeline);
        self.update_state(StreamState::Connected, None).await;

        // Consume frames from broadcast until error or shutdown
        self.streaming_loop(rx).await?;

        Ok(())
    }

    /// Streaming loop: receives frames from the broadcast channel (push model).
    /// Replaces the old pull-based loop that called try_pull_sample every 100ms.
    async fn streaming_loop(
        &mut self,
        mut rx: broadcast::Receiver<Arc<Frame>>,
    ) -> DetectorResult<()> {
        use tokio::sync::broadcast::error::RecvError;

        let mut first_frame = true;
        let camera_id = self.config.camera_id.clone();

        loop {
            // 10-second timeout matches the old "no frames" detection threshold
            match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await {
                Ok(Ok(frame)) => {
                    if first_frame {
                        self.update_state(StreamState::Streaming, None).await;
                        self.reset_backoff();
                        first_frame = false;
                        info!(
                            camera_id = %camera_id,
                            width = frame.width,
                            height = frame.height,
                            "First frame received (push model)"
                        );
                    }

                    self.status.record_frame();

                    // Sampler controls how many frames go to inference (not to live view)
                    if self.sampler.should_sample() {
                        let msg = WorkerMessage::Frame {
                            camera_id: camera_id.clone(),
                            frame: (*frame).clone(), // clone only for inference, live view shares Arc
                        };
                        if self.tx.send(msg).await.is_err() {
                            return Ok(()); // Receiver dropped → shutdown
                        }
                    }
                }

                Ok(Err(RecvError::Lagged(n))) => {
                    // Worker fell behind; skip stale frames — normal under heavy load
                    warn!(camera_id = %camera_id, skipped = n, "Worker lagged, skipping frames");
                }

                Ok(Err(RecvError::Closed)) => {
                    // Broadcast sender dropped (pipeline stopped for unrelated reason)
                    return Err(crate::error::DetectorError::StreamError {
                        camera_id: camera_id.clone(),
                        message: "Frame channel closed unexpectedly".to_string(),
                    });
                }

                Err(_timeout) => {
                    // No frames for 10s — pipeline stalled or camera disconnected
                    return Err(crate::error::DetectorError::StreamError {
                        camera_id: camera_id.clone(),
                        message: "No frames received for 10 seconds".to_string(),
                    });
                }
            }
        }
    }

    async fn update_state(&self, state: StreamState, error: Option<String>) {
        let old_state = self.status.state();
        self.status.set_state(state);
        self.status.set_error(error.clone());

        if old_state != state {
            if state == StreamState::Reconnecting {
                self.status.increment_reconnect();
            }

            let msg = WorkerMessage::StateChanged {
                camera_id: self.config.camera_id.clone(),
                state,
                error,
            };

            let _ = self.tx.send(msg).await;
        }
    }

    fn increase_backoff(&mut self) {
        let max_delay = Duration::from_millis(self.reconnect_config.max_delay_ms);
        let multiplier = self.reconnect_config.backoff_multiplier;

        self.current_delay = std::cmp::min(
            Duration::from_secs_f64(self.current_delay.as_secs_f64() * multiplier),
            max_delay,
        );
    }

    fn reset_backoff(&mut self) {
        self.current_delay = Duration::from_millis(self.reconnect_config.initial_delay_ms);
        self.consecutive_errors = 0;
    }
}

impl Drop for WorkerLoop {
    fn drop(&mut self) {
        if let Some(ref mut pipeline) = self.pipeline {
            let _ = pipeline.stop();
        }
    }
}
