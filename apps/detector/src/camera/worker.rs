//! Camera worker - per-camera processing loop
//!
//! Each camera has its own worker that:
//! - Manages the pipeline lifecycle
//! - Samples frames at configured FPS
//! - Handles reconnection with exponential backoff
//! - Publishes frames to the inference queue

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn, Instrument};

use crate::config::{CameraConfig, ReconnectConfig};
use crate::camera::pipeline::{CameraPipeline, Frame};
use crate::camera::sampler::FrameSampler;
use crate::camera::status::{CameraStatus, StreamState};
use crate::error::DetectorResult;

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

    /// Shutdown sender – kept alive so the receiver is not immediately closed.
    /// Dropping this signals the worker task to stop.
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,

    /// Shutdown receiver passed to the worker task on startup
    shutdown_rx: Option<tokio::sync::oneshot::Receiver<()>>,

    /// Task handle
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl CameraWorker {
    /// Create a new camera worker
    pub fn new(
        config: CameraConfig,
        reconnect_config: ReconnectConfig,
        tx: mpsc::Sender<WorkerMessage>,
        status: Arc<CameraStatus>,
    ) -> Self {
        Self {
            config,
            reconnect_config,
            tx,
            status,
            shutdown_tx: None,
            shutdown_rx: None,
            task_handle: None,
        }
    }
    
    /// Start the worker
    pub fn start(&mut self) -> DetectorResult<()> {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        // Store the sender so it lives as long as the worker.
        // When `stop()` is called (or the worker is dropped) the sender is
        // dropped, which closes the channel and signals the task to exit.
        self.shutdown_tx = Some(shutdown_tx);
        self.shutdown_rx = Some(shutdown_rx);
        
        let config = self.config.clone();
        let reconnect_config = self.reconnect_config.clone();
        let tx = self.tx.clone();
        let status = self.status.clone();
        
        let camera_id = config.camera_id.clone();
        
        let handle = tokio::spawn(async move {
            let mut worker_loop = WorkerLoop::new(
                config,
                reconnect_config,
                tx,
                status,
            );
            
            if let Err(e) = worker_loop.run().await {
                error!(camera_id = %camera_id, error = %e, "Worker loop exited with error");
            }
        }.instrument(tracing::info_span!("camera_worker", camera_id = %self.config.camera_id)));
        
        self.task_handle = Some(handle);
        
        info!(camera_id = %self.config.camera_id, "Worker started");
        
        Ok(())
    }
    
    /// Stop the worker gracefully by signalling via the shutdown channel,
    /// then aborting the task if it doesn't finish promptly.
    pub async fn stop(&mut self) {
        // Drop the sender → closes the channel → worker task receives the signal
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
}

impl WorkerLoop {
    fn new(
        config: CameraConfig,
        reconnect_config: ReconnectConfig,
        tx: mpsc::Sender<WorkerMessage>,
        status: Arc<CameraStatus>,
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
        }
    }
    
    async fn run(&mut self) -> DetectorResult<()> {
        loop {
            // Check if camera is disabled
            if !self.config.enabled {
                self.update_state(StreamState::Disabled, None).await;
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            }
            
            // Try to connect and stream
            match self.connect_and_stream().await {
                Ok(()) => {
                    // Normal exit (shutdown requested)
                    break;
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
                    
                    // Check if max retries exceeded
                    if self.consecutive_errors >= self.reconnect_config.max_retries {
                        self.update_state(StreamState::Failed, Some(error_msg)).await;
                        
                        error!(
                            camera_id = %self.config.camera_id,
                            max_retries = self.reconnect_config.max_retries,
                            "Max retries exceeded, marking as failed"
                        );
                        
                        // Wait longer before trying again
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        self.reset_backoff();
                        continue;
                    }
                    
                    // Reconnect with backoff
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
        // Update state
        self.update_state(StreamState::Connecting, None).await;
        
        // Build pipeline
        let mut pipeline = CameraPipeline::new(self.config.clone());
        pipeline.build()?;
        pipeline.start()?;
        
        self.pipeline = Some(pipeline);
        
        // Update state
        self.update_state(StreamState::Connected, None).await;
        
        // Main streaming loop
        self.streaming_loop().await?;
        
        Ok(())
    }
    
    async fn streaming_loop(&mut self) -> DetectorResult<()> {
        let mut first_frame = true;
        let mut last_frame_time = Instant::now();
        
        loop {
            // Check for pipeline errors - clone the check to avoid borrow conflict
            let is_running = {
                let pipeline = self.pipeline.as_ref().unwrap();
                pipeline.is_running()
            };
            
            if !is_running {
                return Err(crate::error::DetectorError::StreamError {
                    camera_id: self.config.camera_id.clone(),
                    message: "Pipeline stopped unexpectedly".to_string(),
                });
            }
            
            // Try to pull a frame
            let frame_result = {
                let pipeline = self.pipeline.as_ref().unwrap();
                pipeline.pull_frame()
            };
            
            match frame_result {
                Ok(Some(frame)) => {
                    // First frame received
                    if first_frame {
                        self.update_state(StreamState::Streaming, None).await;
                        self.reset_backoff();
                        first_frame = false;
                        
                        info!(
                            camera_id = %self.config.camera_id,
                            width = frame.width,
                            height = frame.height,
                            "First frame received"
                        );
                    }
                    
                    // Update status
                    self.status.record_frame();
                    last_frame_time = Instant::now();
                    
                    // Check if we should sample this frame
                    if self.sampler.should_sample() {
                        // Send frame for inference
                        let msg = WorkerMessage::Frame {
                            camera_id: self.config.camera_id.clone(),
                            frame,
                        };
                        
                        if self.tx.send(msg).await.is_err() {
                            warn!(
                                camera_id = %self.config.camera_id,
                                "Failed to send frame, receiver dropped"
                            );
                            return Ok(()); // Shutdown
                        }
                    }
                }
                Ok(None) => {
                    // No frame available, check for timeout
                    if last_frame_time.elapsed() > Duration::from_secs(10) {
                        return Err(crate::error::DetectorError::StreamError {
                            camera_id: self.config.camera_id.clone(),
                            message: "No frames received for 10 seconds".to_string(),
                        });
                    }
                    
                    // Small sleep to prevent busy loop
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
    }
    
    async fn update_state(&self, state: StreamState, error: Option<String>) {
        let old_state = self.status.state();
        self.status.set_state(state);
        self.status.set_error(error.clone());
        
        // Only send message if state actually changed
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
