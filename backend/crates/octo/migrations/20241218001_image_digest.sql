-- Add image_digest column to sessions table
-- Stores the image digest (sha256) when the container was created/started
-- This allows detecting when a newer image version is available

ALTER TABLE sessions ADD COLUMN image_digest TEXT;

-- Index for finding sessions by image digest (for upgrade detection)
CREATE INDEX IF NOT EXISTS idx_sessions_image_digest ON sessions(image_digest);
