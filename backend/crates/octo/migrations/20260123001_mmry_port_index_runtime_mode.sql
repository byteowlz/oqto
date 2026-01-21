-- In local multi-user mode, mmry_port can be shared across multiple active sessions
-- for the same platform user (per-user mmry instance). In container mode, mmry_port
-- remains per-session and must be unique for active sessions.

DROP INDEX IF EXISTS idx_sessions_active_mmry_port;

CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_active_mmry_port
ON sessions(mmry_port)
WHERE runtime_mode = 'container'
  AND status IN ('pending', 'starting', 'running')
  AND mmry_port IS NOT NULL;
