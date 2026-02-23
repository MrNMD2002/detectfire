//! Camera manager
//!
//! Manages all camera workers, handles frame routing to inference,
//! and coordinates with other components.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use parking_lot::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::{CameraConfig, ReconnectConfig};
use crate::camera::pipeline::{CameraPipeline, Frame};
use crate::camera::worker::{CameraWorker, WorkerMessage};
use crate::camera::status::{CameraStatus, CameraStatusSnapshot, StreamState};
use crate::inference::InferenceEngine;
use crate::decision::DecisionEngine;
use crate::event::EventPublisher;
use crate::error::DetectorResult;

/// Manages all camera workers
pub struct CameraManager {
    /// Camera configurations
    configs: Vec<CameraConfig>,
    
    /// Reconnection config
    reconnect_config: ReconnectConfig,
    
    /// Camera workers by camera_id
    workers: RwLock<HashMap<String, CameraWorker>>,
    
    /// Camera statuses by camera_id
    statuses: Arc<RwLock<HashMap<String, Arc<CameraStatus>>>>,
    
    /// Inference engine
    inference_engine: InferenceEngine,
    
    /// Decision engine
    decision_engine: DecisionEngine,
    
    /// Event publisher
    event_publisher: EventPublisher,
    
    /// Message channel for worker communication
    message_tx: mpsc::Sender<WorkerMessage>,
    message_rx: RwLock<Option<mpsc::Receiver<WorkerMessage>>>,
    
    /// Processing task handle
    processing_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl CameraManager {
    /// Create a new camera manager
    pub fn new(
        configs: Vec<CameraConfig>,
        inference_engine: InferenceEngine,
        decision_engine: DecisionEngine,
        event_publisher: EventPublisher,
    ) -> DetectorResult<Self> {
        // Initialize GStreamer
        CameraPipeline::init_gstreamer()?;
        
        // Create message channel
        let (tx, rx) = mpsc::channel(100);
        
        // Create status map
        let mut statuses = HashMap::new();
        for config in &configs {
            let status = Arc::new(CameraStatus::new(
                &config.camera_id,
                &config.site_id,
                &config.name,
            ));
            statuses.insert(config.camera_id.clone(), status);
        }
        
        Ok(Self {
            configs,
            reconnect_config: ReconnectConfig::default(),
            workers: RwLock::new(HashMap::new()),
            statuses: Arc::new(RwLock::new(statuses)),
            inference_engine,
            decision_engine,
            event_publisher,
            message_tx: tx,
            message_rx: RwLock::new(Some(rx)),
            processing_handle: RwLock::new(None),
        })
    }
    
    /// Set reconnection configuration
    pub fn with_reconnect_config(mut self, config: ReconnectConfig) -> Self {
        self.reconnect_config = config;
        self
    }
    
    /// Start all camera workers
    pub async fn start_all(&self) -> DetectorResult<()> {
        info!(cameras = self.configs.len(), "Starting all camera workers");
        
        // Start message processing task
        self.start_processing_task().await;
        
        // Start workers for each camera
        let mut workers = self.workers.write();
        let statuses = self.statuses.read();
        
        for config in &self.configs {
            if !config.enabled {
                debug!(camera_id = %config.camera_id, "Camera disabled, skipping");
                continue;
            }
            
            let status = statuses
                .get(&config.camera_id)
                .cloned()
                .unwrap();
            
            let mut worker = CameraWorker::new(
                config.clone(),
                self.reconnect_config.clone(),
                self.message_tx.clone(),
                status,
            );
            
            if let Err(e) = worker.start() {
                error!(
                    camera_id = %config.camera_id,
                    error = %e,
                    "Failed to start worker"
                );
                continue;
            }
            
            workers.insert(config.camera_id.clone(), worker);
        }
        
        info!(
            started = workers.len(),
            total = self.configs.len(),
            "Camera workers started"
        );
        
        Ok(())
    }
    
    /// Stop all camera workers
    pub async fn stop_all(&self) {
        info!("Stopping all camera workers");
        
        // Stop processing task
        if let Some(handle) = self.processing_handle.write().take() {
            handle.abort();
            let _ = handle.await;
        }
        
        // Stop all workers
        let mut workers = self.workers.write();
        for (camera_id, worker) in workers.iter_mut() {
            debug!(camera_id = %camera_id, "Stopping worker");
            worker.stop().await;
        }
        workers.clear();
        
        info!("All camera workers stopped");
    }
    
    /// Start a single camera worker
    pub async fn start_camera(&self, camera_id: &str) -> DetectorResult<()> {
        let config = self.configs
            .iter()
            .find(|c| c.camera_id == camera_id)
            .ok_or_else(|| crate::error::DetectorError::CameraNotFound {
                camera_id: camera_id.to_string(),
            })?
            .clone();
        
        let status = self.statuses
            .read()
            .get(camera_id)
            .cloned()
            .unwrap();
        
        let mut worker = CameraWorker::new(
            config,
            self.reconnect_config.clone(),
            self.message_tx.clone(),
            status,
        );
        
        worker.start()?;
        
        self.workers.write().insert(camera_id.to_string(), worker);
        
        info!(camera_id = %camera_id, "Camera started");
        
        Ok(())
    }
    
    /// Stop a single camera worker
    pub async fn stop_camera(&self, camera_id: &str) -> DetectorResult<()> {
        if let Some(mut worker) = self.workers.write().remove(camera_id) {
            worker.stop().await;
            info!(camera_id = %camera_id, "Camera stopped");
            Ok(())
        } else {
            Err(crate::error::DetectorError::CameraNotFound {
                camera_id: camera_id.to_string(),
            })
        }
    }
    
    /// Get status of a camera
    pub fn get_status(&self, camera_id: &str) -> Option<CameraStatusSnapshot> {
        self.statuses
            .read()
            .get(camera_id)
            .map(|s| s.snapshot())
    }
    
    /// Get status of all cameras
    pub fn get_all_statuses(&self) -> Vec<CameraStatusSnapshot> {
        self.statuses
            .read()
            .values()
            .map(|s| s.snapshot())
            .collect()
    }
    
    /// Get camera count by state
    pub fn count_by_state(&self) -> HashMap<StreamState, usize> {
        let mut counts = HashMap::new();
        
        for status in self.statuses.read().values() {
            *counts.entry(status.state()).or_insert(0) += 1;
        }
        
        counts
    }
    
    /// Start the message processing task
    async fn start_processing_task(&self) {
        let rx = self.message_rx.write().take();
        
        if let Some(mut rx) = rx {
            let inference_engine = self.inference_engine.clone();
            let decision_engine = self.decision_engine.clone();
            let event_publisher = self.event_publisher.clone();
            let statuses = self.statuses.clone();
            let configs: HashMap<String, CameraConfig> = self.configs
                .iter()
                .map(|c| (c.camera_id.clone(), c.clone()))
                .collect();
            
            let handle = tokio::spawn(async move {
                while let Some(msg) = rx.recv().await {
                    match msg {
                        WorkerMessage::Frame { camera_id, frame } => {
                            // Get camera config
                            let config = match configs.get(&camera_id) {
                                Some(c) => c,
                                None => continue,
                            };
                            
                            // Run inference
                            let start = std::time::Instant::now();
                            let detections = match inference_engine.detect(&frame, config.imgsz).await {
                                Ok(d) => d,
                                Err(e) => {
                                    error!(
                                        camera_id = %camera_id,
                                        error = %e,
                                        "Inference failed"
                                    );
                                    continue;
                                }
                            };
                            let _inference_ms = start.elapsed().as_millis() as f32;
                            
                            // Update status
                            if let Some(status) = statuses.read().get(&camera_id) {
                                status.record_inference();
                            }
                            
                            // Run decision engine
                            if let Some(event) = decision_engine.process(
                                &camera_id,
                                detections,
                                config,
                            ).await {
                                // Publish event
                                if let Err(e) = event_publisher.publish(event, &frame).await {
                                    error!(
                                        camera_id = %camera_id,
                                        error = %e,
                                        "Failed to publish event"
                                    );
                                }
                            }
                        }
                        WorkerMessage::StateChanged { camera_id, state, error } => {
                            info!(
                                camera_id = %camera_id,
                                state = %state,
                                error = ?error,
                                "Camera state changed"
                            );
                            
                            // Handle stream down/up events
                            match state {
                                StreamState::Reconnecting if error.is_some() => {
                                    // Emit stream_down event after 3 failed attempts
                                    // Drop lock guard before await to avoid Send trait issues
                                    let should_emit = {
                                        if let Some(status) = statuses.read().get(&camera_id) {
                                            status.reconnect_count() == 3
                                        } else {
                                            false
                                        }
                                    };
                                    if should_emit {
                                        let _ = event_publisher.publish_stream_down(
                                            &camera_id,
                                            error.as_deref().unwrap_or("Unknown error"),
                                        ).await;
                                    }
                                }
                                StreamState::Streaming => {
                                    // Emit stream_up event
                                    let _ = event_publisher.publish_stream_up(&camera_id).await;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            });
            
            *self.processing_handle.write() = Some(handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Integration tests would go here
    // Requires GStreamer and test RTSP streams
}
