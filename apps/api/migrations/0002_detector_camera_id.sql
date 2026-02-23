-- Add detector_camera_id for mapping API cameras to detector stream config
ALTER TABLE cameras ADD COLUMN IF NOT EXISTS detector_camera_id VARCHAR(100);

-- Index for stream lookups
CREATE INDEX IF NOT EXISTS idx_cameras_detector_camera_id ON cameras(detector_camera_id);
