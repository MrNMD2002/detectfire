-- Add codec and conf_other columns to cameras table
-- codec: video codec type ("h264" or "h265"), default h264
-- conf_other: confidence threshold for class 2 (fire-related indicators)

ALTER TABLE cameras
    ADD COLUMN IF NOT EXISTS codec VARCHAR(10) NOT NULL DEFAULT 'h264',
    ADD COLUMN IF NOT EXISTS conf_other REAL NOT NULL DEFAULT 0.4;
