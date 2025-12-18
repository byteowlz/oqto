-- Add partial unique index on ports for active sessions to prevent port conflicts
-- This ensures that no two active sessions can have overlapping ports

-- Unique constraint on opencode_port for active sessions
CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_active_opencode_port 
ON sessions(opencode_port) 
WHERE status IN ('pending', 'starting', 'running');

-- Unique constraint on fileserver_port for active sessions
CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_active_fileserver_port 
ON sessions(fileserver_port) 
WHERE status IN ('pending', 'starting', 'running');

-- Unique constraint on ttyd_port for active sessions
CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_active_ttyd_port 
ON sessions(ttyd_port) 
WHERE status IN ('pending', 'starting', 'running');

-- Unique constraint on eavs_port for active sessions (nullable, so only non-null values are constrained)
CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_active_eavs_port 
ON sessions(eavs_port) 
WHERE status IN ('pending', 'starting', 'running') AND eavs_port IS NOT NULL;
