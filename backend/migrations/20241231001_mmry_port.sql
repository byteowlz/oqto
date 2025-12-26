-- Add mmry_port column for memory service integration
ALTER TABLE sessions ADD COLUMN mmry_port INTEGER;

-- Unique constraint on mmry_port for active sessions (nullable, so only non-null values are constrained)
CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_active_mmry_port 
ON sessions(mmry_port) 
WHERE status IN ('pending', 'starting', 'running') AND mmry_port IS NOT NULL;
