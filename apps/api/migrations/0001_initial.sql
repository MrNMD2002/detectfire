-- Initial schema migration

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Users table
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    email VARCHAR(255) NOT NULL UNIQUE,
    password_hash VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL DEFAULT 'user',
    active BOOLEAN NOT NULL DEFAULT true,
    telegram_chat_id VARCHAR(50),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Cameras table
CREATE TABLE cameras (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    site_id VARCHAR(50) NOT NULL,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    rtsp_url_encrypted TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    fps_sample INTEGER NOT NULL DEFAULT 3,
    imgsz INTEGER NOT NULL DEFAULT 640,
    conf_fire REAL NOT NULL DEFAULT 0.5,
    conf_smoke REAL NOT NULL DEFAULT 0.4,
    window_size INTEGER NOT NULL DEFAULT 10,
    fire_hits INTEGER NOT NULL DEFAULT 3,
    smoke_hits INTEGER NOT NULL DEFAULT 3,
    cooldown_sec INTEGER NOT NULL DEFAULT 60,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Events table
CREATE TABLE events (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    event_type VARCHAR(50) NOT NULL,
    camera_id UUID NOT NULL REFERENCES cameras(id) ON DELETE CASCADE,
    site_id VARCHAR(50) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    confidence REAL NOT NULL,
    detections JSONB NOT NULL DEFAULT '[]'::jsonb,
    snapshot_path TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    acknowledged BOOLEAN NOT NULL DEFAULT false,
    acknowledged_by UUID REFERENCES users(id),
    acknowledged_at TIMESTAMPTZ
);

-- Indexes
CREATE INDEX idx_events_camera_id ON events(camera_id);
CREATE INDEX idx_events_site_id ON events(site_id);
CREATE INDEX idx_events_event_type ON events(event_type);
CREATE INDEX idx_events_timestamp ON events(timestamp DESC);
CREATE INDEX idx_events_acknowledged ON events(acknowledged);
CREATE INDEX idx_cameras_site_id ON cameras(site_id);
CREATE INDEX idx_cameras_enabled ON cameras(enabled);

-- Trigger to update updated_at
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_cameras_updated_at
    BEFORE UPDATE ON cameras
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Insert default admin user (password: admin123)
INSERT INTO users (email, password_hash, name, role)
VALUES (
    'admin@example.com',
    '$2b$12$LQv3c1yqBWVHxkd0LGEdoexMYB5Y8OPYfT0REr.GW6QF3tqQVYKem',
    'Administrator',
    'admin'
);
