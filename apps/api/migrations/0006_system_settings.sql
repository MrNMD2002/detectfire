-- System-wide key/value settings (Telegram config, etc.)
-- Persists runtime changes made through the Settings UI without restart.

CREATE TABLE IF NOT EXISTS system_settings (
    key         VARCHAR(100) PRIMARY KEY,
    value       TEXT         NOT NULL,
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);
