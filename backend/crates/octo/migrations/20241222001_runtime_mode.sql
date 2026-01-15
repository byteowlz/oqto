-- Add runtime_mode column to sessions table
-- This column indicates whether the session runs in container mode (Docker/Podman)
-- or local mode (native processes).

-- Add runtime_mode column with default value 'container' for backwards compatibility
ALTER TABLE sessions ADD COLUMN runtime_mode TEXT NOT NULL DEFAULT 'container' 
    CHECK (runtime_mode IN ('container', 'local'));

-- Index for filtering by runtime mode
CREATE INDEX IF NOT EXISTS idx_sessions_runtime_mode ON sessions(runtime_mode);
