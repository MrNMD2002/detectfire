//! Image preprocessing for YOLOv10
//!
//! Handles image resizing, padding, and normalization for model input.

use ndarray::{Array4, s};
use image::{RgbImage, imageops::FilterType};

use crate::camera::Frame;
use crate::error::{DetectorError, DetectorResult};

/// Preprocess a frame for YOLOv10 inference
///
/// # Arguments
/// * `frame` - Input frame (RGB, any size)
/// * `target_size` - Target size (width, height)
///
/// # Returns
/// * Preprocessed tensor [1, 3, H, W] in NCHW format
/// * Scale factor used for letterboxing
pub fn preprocess_frame(
    frame: &Frame,
    target_size: u32,
) -> DetectorResult<(Array4<f32>, f32)> {
    // Convert frame bytes to an image; propagate failure instead of panicking
    let img = frame.to_image().ok_or_else(|| {
        DetectorError::InferenceError(
            "Failed to convert frame buffer to RGB image".to_string(),
        )
    })?;

    Ok(preprocess_image(&img, target_size))
}

/// Preprocess an RGB image for YOLOv10 inference
pub fn preprocess_image(
    img: &RgbImage,
    target_size: u32,
) -> (Array4<f32>, f32) {
    let (orig_w, orig_h) = (img.width(), img.height());
    
    // Calculate scale for letterboxing (maintain aspect ratio)
    let scale = (target_size as f32 / orig_w as f32)
        .min(target_size as f32 / orig_h as f32);
    
    let new_w = (orig_w as f32 * scale) as u32;
    let new_h = (orig_h as f32 * scale) as u32;
    
    // Resize image
    let resized = image::imageops::resize(
        img,
        new_w,
        new_h,
        FilterType::Triangle, // Bilinear - good balance of speed/quality
    );
    
    // Create letterboxed image with padding (gray: 114)
    let mut letterboxed = RgbImage::from_pixel(
        target_size,
        target_size,
        image::Rgb([114, 114, 114]),
    );
    
    // Calculate padding
    let pad_x = (target_size - new_w) / 2;
    let pad_y = (target_size - new_h) / 2;
    
    // Copy resized image onto letterboxed canvas
    for y in 0..new_h {
        for x in 0..new_w {
            let pixel = resized.get_pixel(x, y);
            letterboxed.put_pixel(x + pad_x, y + pad_y, *pixel);
        }
    }
    
    // Convert to tensor [1, 3, H, W]
    let tensor = image_to_tensor(&letterboxed);
    
    (tensor, scale)
}

/// Convert RGB image to normalized NCHW tensor
fn image_to_tensor(img: &RgbImage) -> Array4<f32> {
    let (width, height) = (img.width() as usize, img.height() as usize);
    
    // Create array [1, 3, H, W]
    let mut tensor = Array4::<f32>::zeros((1, 3, height, width));
    
    // Fill tensor with normalized pixel values
    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x as u32, y as u32);
            
            // Normalize to [0, 1]
            tensor[[0, 0, y, x]] = pixel[0] as f32 / 255.0; // R
            tensor[[0, 1, y, x]] = pixel[1] as f32 / 255.0; // G
            tensor[[0, 2, y, x]] = pixel[2] as f32 / 255.0; // B
        }
    }
    
    tensor
}

/// Preprocess multiple frames for batched inference
pub fn preprocess_batch(
    frames: &[Frame],
    target_size: u32,
) -> DetectorResult<(Array4<f32>, Vec<f32>)> {
    let batch_size = frames.len();

    // Preallocate batch tensor
    let mut batch = Array4::<f32>::zeros((
        batch_size,
        3,
        target_size as usize,
        target_size as usize,
    ));

    let mut scales = Vec::with_capacity(batch_size);

    // Process each frame – propagate any conversion error
    for (i, frame) in frames.iter().enumerate() {
        let (tensor, scale) = preprocess_frame(frame, target_size)?;

        // Copy into batch
        batch.slice_mut(s![i, .., .., ..]).assign(&tensor.slice(s![0, .., .., ..]));
        scales.push(scale);
    }

    Ok((batch, scales))
}

/// Calculate preprocessing parameters for a given image size
pub struct PreprocessParams {
    pub scale: f32,
    pub pad_x: u32,
    pub pad_y: u32,
    pub new_w: u32,
    pub new_h: u32,
}

impl PreprocessParams {
    pub fn calculate(orig_w: u32, orig_h: u32, target_size: u32) -> Self {
        let scale = (target_size as f32 / orig_w as f32)
            .min(target_size as f32 / orig_h as f32);
        
        let new_w = (orig_w as f32 * scale) as u32;
        let new_h = (orig_h as f32 * scale) as u32;
        
        let pad_x = (target_size - new_w) / 2;
        let pad_y = (target_size - new_h) / 2;
        
        Self {
            scale,
            pad_x,
            pad_y,
            new_w,
            new_h,
        }
    }
    
    /// Convert letterboxed coordinates back to original image coordinates
    pub fn to_original(&self, x: f32, y: f32, target_size: u32) -> (f32, f32) {
        let size = target_size as f32;
        
        let x_unpadded = x * size - self.pad_x as f32;
        let y_unpadded = y * size - self.pad_y as f32;
        
        let x_orig = x_unpadded / self.scale;
        let y_orig = y_unpadded / self.scale;
        
        (x_orig, y_orig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preprocess_image_square() {
        let img = RgbImage::from_fn(640, 640, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });
        
        let (tensor, scale) = preprocess_image(&img, 640);
        
        assert_eq!(tensor.shape(), &[1, 3, 640, 640]);
        assert!((scale - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_preprocess_image_landscape() {
        let img = RgbImage::from_fn(1280, 720, |_, _| {
            image::Rgb([128, 128, 128])
        });
        
        let (tensor, scale) = preprocess_image(&img, 640);
        
        assert_eq!(tensor.shape(), &[1, 3, 640, 640]);
        // Scale should be 640/1280 = 0.5
        assert!((scale - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_preprocess_image_portrait() {
        let img = RgbImage::from_fn(720, 1280, |_, _| {
            image::Rgb([128, 128, 128])
        });
        
        let (tensor, scale) = preprocess_image(&img, 640);
        
        assert_eq!(tensor.shape(), &[1, 3, 640, 640]);
        // Scale should be 640/1280 = 0.5
        assert!((scale - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_preprocess_params() {
        let params = PreprocessParams::calculate(1920, 1080, 640);
        
        // Scale should be 640/1920 = 0.333...
        assert!((params.scale - 0.333).abs() < 0.01);
        
        // New height should be 1080 * 0.333 = 360
        assert!(params.new_h < 640);
        
        // Should have vertical padding
        assert!(params.pad_y > 0);
    }

    #[test]
    fn test_tensor_normalization() {
        // Create image with known pixel values
        let img = RgbImage::from_fn(2, 2, |_, _| {
            image::Rgb([255, 128, 0])
        });
        
        let tensor = image_to_tensor(&img);
        
        // Check normalization
        assert!((tensor[[0, 0, 0, 0]] - 1.0).abs() < 0.01); // R = 255 -> 1.0
        assert!((tensor[[0, 1, 0, 0]] - 0.5).abs() < 0.01); // G = 128 -> 0.5
        assert!((tensor[[0, 2, 0, 0]] - 0.0).abs() < 0.01); // B = 0 -> 0.0
    }
}
