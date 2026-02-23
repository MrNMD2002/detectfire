//! ONNX Runtime inference engine
//!
//! Manages the ONNX Runtime session and performs inference on frames.

use std::sync::Arc;
use std::time::Instant;
use std::path::PathBuf;
use parking_lot::RwLock;
use ndarray::Array4;
use ort::{
    session::Session,
    value::Tensor,
};
use tracing::{debug, info, warn};

use crate::config::InferenceConfig;
use crate::camera::Frame;
use crate::error::{DetectorError, DetectorResult};

use super::detection::{Detection, InferenceResult};
use super::preprocess::{preprocess_frame, PreprocessParams};
use super::postprocess::{postprocess, postprocess_with_probs};

/// ONNX Runtime inference engine
#[derive(Clone)]
pub struct InferenceEngine {
    inner: Arc<InferenceEngineInner>,
}

struct InferenceEngineInner {
    /// ONNX Runtime session
    session: RwLock<Session>,
    
    /// Model configuration
    config: InferenceConfig,
    
    /// Number of classes
    num_classes: usize,
    
    /// Warmup completed
    warmed_up: RwLock<bool>,
    
    /// Output processor
    output_processor: OutputProcessor,
}

impl InferenceEngine {
    /// Create a new inference engine
    pub fn new(config: &InferenceConfig) -> DetectorResult<Self> {
        info!(
            model_path = %config.model_path,
            device = %config.device,
            "Initializing inference engine"
        );
        
        // Configure execution providers
        let mut session_builder = Session::builder()
            .map_err(|e| DetectorError::ModelLoadError(format!("Failed to create session builder: {}", e)))?;
        
        session_builder = session_builder
            .with_intra_threads(config.num_threads)
            .map_err(|e| DetectorError::ModelLoadError(format!("Failed to set thread count: {}", e)))?;
        
        // Add CUDA execution provider if requested (ort 2.0 API: ort::ep::CUDA)
        if config.device.starts_with("cuda") {
            let device_id = config.device
                .strip_prefix("cuda:")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0);

            #[cfg(feature = "gpu")]
            {
                use ort::ep;
                match session_builder.with_execution_providers([
                    ep::CUDA::default().build(),
                ]) {
                    Ok(builder) => {
                        session_builder = builder;
                        info!(device_id = device_id, "CUDA execution provider registered");
                    }
                    Err(e) => {
                        warn!(error = %e, "CUDA EP registration failed, falling back to CPU");
                    }
                }
            }
            #[cfg(not(feature = "gpu"))]
            {
                warn!("CUDA requested but not available (compiled without gpu feature)");
            }
        }
        
        // Resolve model path
        let model_path = if config.model_path.starts_with('/') {
            PathBuf::from(&config.model_path)
        } else {
            let app_path = PathBuf::from("/app").join(&config.model_path);
            if app_path.exists() {
                app_path
            } else {
                PathBuf::from(&config.model_path)
            }
        };
        
        if !model_path.exists() {
            return Err(DetectorError::ModelLoadError(format!(
                "Model file not found: {}", model_path.display()
            )));
        }
        
        let model_data = std::fs::read(&model_path)
            .map_err(|e| DetectorError::ModelLoadError(format!("Failed to read model: {}", e)))?;
        
        let session = session_builder
            .commit_from_memory(&model_data)
            .map_err(|e| DetectorError::ModelLoadError(format!("Failed to load model: {}", e)))?;
        
        // YOLOv26 (SalahALHaismawi): fire (0), other (1), smoke (2)
        let num_classes = 3;
        
        let inner = InferenceEngineInner {
            session: RwLock::new(session),
            config: config.clone(),
            num_classes,
            warmed_up: RwLock::new(false),
            output_processor: OutputProcessor::new(num_classes),
        };
        
        let engine = Self {
            inner: Arc::new(inner),
        };
        
        // Warmup with default size (e.g. 640) can be adjusted if needed
        if config.warmup_frames > 0 {
            engine.warmup(config.warmup_frames, 640)?;
        }
        
        Ok(engine)
    }
    
    /// Warmup the model with dummy inference
    fn warmup(&self, num_frames: usize, input_size: u32) -> DetectorResult<()> {
        info!(frames = num_frames, size = input_size, "Warming up inference engine");
        
        let dummy_frame = Frame {
            data: vec![128u8; (input_size * input_size * 3) as usize],
            width: input_size,
            height: input_size,
            timestamp: 0,
        };
        
        for i in 0..num_frames {
            let start = Instant::now();
            let _ = self.run_inference(&dummy_frame, input_size, 0.5, 0.5, 0.5)?;
            debug!(iteration = i + 1, elapsed_ms = start.elapsed().as_millis(), "Warmup iteration");
        }
        
        *self.inner.warmed_up.write() = true;
        info!("Warmup completed");
        
        Ok(())
    }
    
    /// Run inference with input size matching frame or config
    pub async fn detect(&self, frame: &Frame, input_size: u32) -> DetectorResult<InferenceResult> {
        // Use default thresholds, actual filtering happens in decision engine
        self.run_inference(frame, input_size, 0.1, 0.1, 0.1)
    }
    
    pub fn run_inference(
        &self,
        frame: &Frame,
        input_size: u32,
        conf_fire: f32,
        conf_smoke: f32,
        conf_other: f32,
    ) -> DetectorResult<InferenceResult> {
        // Preprocess
        let preprocess_start = Instant::now();
        let (input_tensor, scale) = preprocess_frame(frame, input_size);
        let preprocess_ms = preprocess_start.elapsed().as_secs_f32() * 1000.0;
        
        // Run inference
        let inference_start = Instant::now();
        let output = self.run_session(&input_tensor)?;
        let inference_ms = inference_start.elapsed().as_secs_f32() * 1000.0;
        
        // Postprocess using the reusable processor
        let postprocess_start = Instant::now();
        let detections = self.inner.output_processor.process(
            &output,
            conf_fire,
            conf_smoke,
            conf_other,
            scale,
        )?;
        let postprocess_ms = postprocess_start.elapsed().as_secs_f32() * 1000.0;
        
        debug!(
            preprocess_ms = preprocess_ms,
            inference_ms = inference_ms,
            postprocess_ms = postprocess_ms,
            detections = detections.len(),
            "Inference completed"
        );
        
        Ok(InferenceResult {
            detections,
            preprocess_ms,
            inference_ms,
            postprocess_ms,
        })
    }
    
    fn run_session(&self, input: &Array4<f32>) -> DetectorResult<Vec<f32>> {
        let shape_slice: &[usize] = input.shape();
        let data_vec: Vec<f32> = input.iter().copied().collect();
        
        let input_tensor = Tensor::from_array((shape_slice, data_vec))
            .map_err(|e| DetectorError::InferenceError(format!("Failed to create input tensor: {}", e)))?;
        
        let mut session = self.inner.session.write();
        let outputs = session
            .run(ort::inputs![input_tensor])
            .map_err(|e| DetectorError::InferenceError(format!("Inference failed: {}", e)))?;
        
        let output = &outputs[0];
        let (_, tensor_data) = output.try_extract_tensor::<f32>()
            .map_err(|e| DetectorError::InferenceError(format!("Failed to extract output: {}", e)))?;

        Ok(tensor_data.iter().copied().collect())
    }
    
    pub fn is_warmed_up(&self) -> bool {
        *self.inner.warmed_up.read()
    }
}

/// Helper struct to encapsulate model output processing logic (OOP/Strategy Pattern)
struct OutputProcessor {
    num_classes: usize,
    iou_threshold: f32,
}

impl OutputProcessor {
    pub fn new(num_classes: usize) -> Self {
        Self {
            num_classes,
            iou_threshold: 0.45, // Standard default
        }
    }
    
    pub fn process(
        &self,
        output: &[f32],
        conf_fire: f32,
        conf_smoke: f32,
        conf_other: f32,
        scale: f32,
    ) -> DetectorResult<Vec<Detection>> {
        let output_len = output.len();
        if output_len > 10000 {
            self.process_standard_yolo(output, conf_fire, conf_smoke, conf_other)
        } else {
            self.process_end_to_end(output, conf_fire, conf_smoke, conf_other, scale)
        }
    }

    fn process_standard_yolo(
        &self,
        output: &[f32],
        conf_fire: f32,
        conf_smoke: f32,
        conf_other: f32,
    ) -> DetectorResult<Vec<Detection>> {
        let num_detections = output.len() / (4 + self.num_classes);
        let shape = (4 + self.num_classes, num_detections);
        let output_array = ndarray::Array2::from_shape_vec(shape, output.to_vec())
            .map_err(|e| DetectorError::InferenceError(format!("Failed to reshape output: {}", e)))?;
        Ok(postprocess_with_probs(
            output_array.view(),
            conf_fire,
            conf_smoke,
            conf_other,
            self.iou_threshold,
            self.num_classes,
        ))
    }

    fn process_end_to_end(
        &self,
        output: &[f32],
        conf_fire: f32,
        conf_smoke: f32,
        conf_other: f32,
        scale: f32,
    ) -> DetectorResult<Vec<Detection>> {
        let num_detections = output.len() / 6;
        let shape = (num_detections, 6);
        let output_array = ndarray::Array2::from_shape_vec(shape, output.to_vec())
            .map_err(|e| DetectorError::InferenceError(format!("Failed to reshape output: {}", e)))?;
        Ok(postprocess(
            output_array.view(),
            conf_fire,
            conf_smoke,
            conf_other,
            self.iou_threshold,
            scale,
        ))
    }
}

#[cfg(test)]
mod tests {
    // Tests...
}
