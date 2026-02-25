//! Camera config sync
//!
//! Writes cameras.yaml from the current DB camera list so the detector can
//! reload its configuration via the gRPC reload_config() call.

use anyhow::Result;
use serde::Serialize;
use tracing::{info, warn};

use crate::models::Camera;

/// Minimal camera entry that matches the detector's CameraConfig serde format.
#[derive(Serialize)]
struct CameraEntry<'a> {
    camera_id: &'a str,
    site_id: &'a str,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    rtsp_url: &'a str,
    enabled: bool,
    codec: &'a str,
    fps_sample: u32,
    imgsz: u32,
    conf_fire: f32,
    conf_smoke: f32,
    conf_other: f32,
    window_size: u32,
    fire_hits: u32,
    smoke_hits: u32,
    cooldown_sec: u64,
}

#[derive(Serialize)]
struct CamerasYaml<'a> {
    cameras: Vec<CameraEntry<'a>>,
}

/// Write cameras.yaml from the current list of cameras (with decrypted RTSP URLs).
///
/// Only cameras that have a `detector_camera_id` are written — cameras without one
/// cannot be mapped to a detector worker and are skipped with a warning.
pub fn write_cameras_yaml(cameras: &[Camera]) -> Result<()> {
    let config_dir = std::env::var("CONFIG_DIR")
        .unwrap_or_else(|_| "../../configs".to_string());

    let path = std::path::Path::new(&config_dir).join("cameras.yaml");

    let entries: Vec<CameraEntry> = cameras
        .iter()
        .filter_map(|cam| {
            let camera_id = match cam.detector_camera_id.as_deref() {
                Some(id) if !id.is_empty() => id,
                _ => {
                    warn!(
                        camera_id = %cam.id,
                        name = %cam.name,
                        "Camera has no detector_camera_id, skipping in cameras.yaml"
                    );
                    return None;
                }
            };

            Some(CameraEntry {
                camera_id,
                site_id: &cam.site_id,
                name: &cam.name,
                description: cam.description.as_deref(),
                rtsp_url: &cam.rtsp_url, // decrypted plaintext from DB
                enabled: cam.enabled,
                codec: &cam.codec,
                fps_sample: cam.fps_sample,
                imgsz: cam.imgsz,
                conf_fire: cam.conf_fire,
                conf_smoke: cam.conf_smoke,
                conf_other: cam.conf_other,
                window_size: cam.window_size,
                fire_hits: cam.fire_hits,
                smoke_hits: cam.smoke_hits,
                cooldown_sec: cam.cooldown_sec,
            })
        })
        .collect();

    let written = entries.len();
    let yaml_doc = CamerasYaml { cameras: entries };
    let content = serde_yaml::to_string(&yaml_doc)?;

    std::fs::write(&path, content.as_bytes())?;

    info!(
        path = %path.display(),
        cameras_written = written,
        cameras_total = cameras.len(),
        "Wrote cameras.yaml for detector reload"
    );

    Ok(())
}
