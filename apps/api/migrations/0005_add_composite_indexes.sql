-- Composite indexes for common query patterns
-- EventsPage: WHERE event_type = ? ORDER BY timestamp DESC LIMIT N OFFSET M
CREATE INDEX IF NOT EXISTS idx_events_type_timestamp
    ON events(event_type, timestamp DESC);

-- Camera-specific event list (CamerasPage snapshot / EventsPage filter by camera)
CREATE INDEX IF NOT EXISTS idx_events_camera_timestamp
    ON events(camera_id, timestamp DESC);
