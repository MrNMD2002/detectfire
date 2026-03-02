//! Post-processing for YOLOv10 output
//!
//! Handles decoding model output, NMS, and converting to Detection objects.

use ndarray::{Array2, ArrayView2, Axis};

use super::detection::{Detection, DetectionClass, BoundingBox};

/// Post-process YOLOv10/YOLOv26 output (end-to-end format)
///
/// # Arguments
/// * `output` - Model output [num_detections, 6]: x1, y1, x2, y2, confidence, class_id (0=fire, 1=other, 2=smoke)
/// * `conf_fire` - Confidence threshold for fire
/// * `conf_smoke` - Confidence threshold for smoke
/// * `conf_other` - Confidence threshold for other (fire-related indicators)
/// * `iou_thresh` - IoU threshold for NMS
/// * `scale` - Scale factor from preprocessing
pub fn postprocess(
    output: ArrayView2<f32>,
    conf_fire: f32,
    conf_smoke: f32,
    conf_other: f32,
    iou_thresh: f32,
    _scale: f32,
) -> Vec<Detection> {
    let num_detections = output.shape()[0];
    let mut detections = Vec::new();

    for i in 0..num_detections {
        let det = output.row(i);
        // as_slice() returns None for non-contiguous views; fall back to owned Vec
        let det_slice: Vec<f32>;
        let det_data: &[f32] = match det.as_slice() {
            Some(s) => s,
            None => {
                det_slice = det.iter().copied().collect();
                &det_slice
            }
        };
        let (bbox, confidence, class_id) = parse_detection(det_data);

        let class = match DetectionClass::from_index(class_id) {
            Some(c) => c,
            None => continue,
        };

        let threshold = match class {
            DetectionClass::Fire => conf_fire,
            DetectionClass::Smoke => conf_smoke,
            DetectionClass::Other => conf_other,
        };

        if confidence < threshold {
            continue;
        }

        detections.push(Detection::new(class, confidence, bbox));
    }

    let fire_detections: Vec<_> = detections.iter().filter(|d| d.class == DetectionClass::Fire).cloned().collect();
    let smoke_detections: Vec<_> = detections.iter().filter(|d| d.class == DetectionClass::Smoke).cloned().collect();
    let other_detections: Vec<_> = detections.iter().filter(|d| d.class == DetectionClass::Other).cloned().collect();

    let mut final_detections = nms(fire_detections, iou_thresh);
    final_detections.extend(nms(smoke_detections, iou_thresh));
    final_detections.extend(nms(other_detections, iou_thresh));

    final_detections
}

/// Parse a single detection from raw output
fn parse_detection(det: &[f32]) -> (BoundingBox, f32, usize) {
    // Format: [x1, y1, x2, y2, confidence, class_id]
    // YOLOv10/v26 end-to-end NMS models output coordinates in pixel space
    // for the model input size (640×640). Normalize to [0, 1] before storing.
    // (process_standard_yolo already divides by 640 in postprocess_with_probs)

    if det.len() >= 6 {
        const MODEL_INPUT_SIZE: f32 = 640.0;
        let x1 = (det[0] / MODEL_INPUT_SIZE).clamp(0.0, 1.0);
        let y1 = (det[1] / MODEL_INPUT_SIZE).clamp(0.0, 1.0);
        let x2 = (det[2] / MODEL_INPUT_SIZE).clamp(0.0, 1.0);
        let y2 = (det[3] / MODEL_INPUT_SIZE).clamp(0.0, 1.0);
        let confidence = det[4];
        let class_id = det[5] as usize;

        let bbox = BoundingBox::new(x1, y1, x2 - x1, y2 - y1);

        (bbox, confidence, class_id)
    } else {
        // Fallback for unexpected format
        (BoundingBox::new(0.0, 0.0, 0.0, 0.0), 0.0, 0)
    }
}

/// Non-Maximum Suppression
fn nms(mut detections: Vec<Detection>, iou_thresh: f32) -> Vec<Detection> {
    if detections.is_empty() {
        return detections;
    }
    
    // Sort by confidence (descending). NaN values sort last.
    detections.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    
    let mut keep = Vec::new();
    let mut suppressed = vec![false; detections.len()];
    
    for i in 0..detections.len() {
        if suppressed[i] {
            continue;
        }
        
        keep.push(detections[i].clone());
        
        for j in (i + 1)..detections.len() {
            if suppressed[j] {
                continue;
            }
            
            let iou = detections[i].bbox.iou(&detections[j].bbox);
            if iou > iou_thresh {
                suppressed[j] = true;
            }
        }
    }
    
    keep
}

/// Postprocess with transposed output
pub fn postprocess_transposed(
    output: ArrayView2<f32>,
    conf_fire: f32,
    conf_smoke: f32,
    conf_other: f32,
    iou_thresh: f32,
    scale: f32,
) -> Vec<Detection> {
    let transposed = output.t();
    postprocess(transposed, conf_fire, conf_smoke, conf_other, iou_thresh, scale)
}

/// Decode YOLO output with class probabilities (standard format)
/// Output format: [num_classes + 4, num_detections]; first 4 rows = bbox (cx, cy, w, h), rest = class probs
pub fn postprocess_with_probs(
    output: ArrayView2<f32>,
    conf_fire: f32,
    conf_smoke: f32,
    conf_other: f32,
    iou_thresh: f32,
    num_classes: usize,
) -> Vec<Detection> {
    let num_dims = output.shape()[0];
    let num_detections = output.shape()[1];
    
    if num_dims != 4 + num_classes {
        // Unexpected format
        return Vec::new();
    }
    
    let mut detections = Vec::new();
    
    for i in 0..num_detections {
        // Extract bbox (cx, cy, w, h)
        let cx = output[[0, i]];
        let cy = output[[1, i]];
        let w = output[[2, i]];
        let h = output[[3, i]];
        
        // Find best class
        let mut best_class = 0;
        let mut best_prob = 0.0f32;
        
        for c in 0..num_classes {
            let prob = output[[4 + c, i]];
            if prob > best_prob {
                best_prob = prob;
                best_class = c;
            }
        }
        
        let class = match DetectionClass::from_index(best_class) {
            Some(c) => c,
            None => continue,
        };

        let threshold = match class {
            DetectionClass::Fire => conf_fire,
            DetectionClass::Smoke => conf_smoke,
            DetectionClass::Other => conf_other,
        };

        if best_prob < threshold {
            continue;
        }
        
        // Convert to normalized bbox
        // Assuming output is in pixel coords for 640x640
        let bbox = BoundingBox::from_center(
            cx / 640.0,
            cy / 640.0,
            w / 640.0,
            h / 640.0,
        );
        
        detections.push(Detection::new(class, best_prob, bbox));
    }
    
    // Apply NMS per class
    let fire_detections: Vec<_> = detections
        .iter()
        .filter(|d| d.class == DetectionClass::Fire)
        .cloned()
        .collect();
    
    let smoke_detections: Vec<_> = detections.iter().filter(|d| d.class == DetectionClass::Smoke).cloned().collect();
    let other_detections: Vec<_> = detections.iter().filter(|d| d.class == DetectionClass::Other).cloned().collect();

    let mut final_detections = nms(fire_detections, iou_thresh);
    final_detections.extend(nms(smoke_detections, iou_thresh));
    final_detections.extend(nms(other_detections, iou_thresh));

    final_detections
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::Array2;

    #[test]
    fn test_nms_basic() {
        let detections = vec![
            Detection::new(DetectionClass::Fire, 0.9, BoundingBox::new(0.1, 0.1, 0.2, 0.2)),
            Detection::new(DetectionClass::Fire, 0.8, BoundingBox::new(0.11, 0.11, 0.2, 0.2)), // Overlaps
            Detection::new(DetectionClass::Fire, 0.7, BoundingBox::new(0.5, 0.5, 0.2, 0.2)), // Separate
        ];
        
        let result = nms(detections, 0.5);
        
        // Should keep 2 (highest confidence from overlapping pair + separate one)
        assert_eq!(result.len(), 2);
        assert!((result[0].confidence - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_nms_no_overlap() {
        let detections = vec![
            Detection::new(DetectionClass::Fire, 0.9, BoundingBox::new(0.0, 0.0, 0.1, 0.1)),
            Detection::new(DetectionClass::Fire, 0.8, BoundingBox::new(0.5, 0.5, 0.1, 0.1)),
            Detection::new(DetectionClass::Fire, 0.7, BoundingBox::new(0.9, 0.9, 0.1, 0.1)),
        ];
        
        let result = nms(detections, 0.5);
        
        // All should be kept
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_parse_detection() {
        let det = [0.1, 0.2, 0.3, 0.4, 0.85, 0.0]; // x1, y1, x2, y2, conf, class
        
        let (bbox, conf, class) = parse_detection(&det);
        
        assert!((bbox.x - 0.1).abs() < 0.001);
        assert!((bbox.y - 0.2).abs() < 0.001);
        assert!((bbox.width - 0.2).abs() < 0.001);
        assert!((bbox.height - 0.2).abs() < 0.001);
        assert!((conf - 0.85).abs() < 0.001);
        assert_eq!(class, 0);
    }

    #[test]
    fn test_postprocess_filters_by_confidence() {
        // Create output with detections at different confidences
        let output = Array2::from_shape_vec((3, 6), vec![
            0.1, 0.1, 0.3, 0.3, 0.9, 0.0, // Fire, high conf
            0.4, 0.4, 0.6, 0.6, 0.3, 0.0, // Fire, low conf
            0.7, 0.7, 0.9, 0.9, 0.8, 1.0, // Smoke, high conf
        ]).unwrap();
        
        let result = postprocess(output.view(), 0.5, 0.5, 0.5, 0.45, 1.0);
        
        // Should only keep 2 high confidence detections
        assert_eq!(result.len(), 2);
    }
}
