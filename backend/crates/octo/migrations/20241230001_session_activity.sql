-- Add last_activity_at column for idle timeout tracking
ALTER TABLE sessions ADD COLUMN last_activity_at TEXT;

-- Set initial value to started_at for existing running sessions
UPDATE sessions SET last_activity_at = started_at WHERE status = 'running' AND last_activity_at IS NULL;

-- Index for finding idle sessions
CREATE INDEX IF NOT EXISTS idx_sessions_last_activity ON sessions(last_activity_at);
