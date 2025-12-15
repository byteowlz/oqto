-- Initial database schema for workspace-backend

-- Sessions table: tracks active and historical container sessions
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    -- Container information
    container_id TEXT,
    container_name TEXT NOT NULL,
    -- User/ownership
    user_id TEXT NOT NULL DEFAULT 'default',
    -- Workspace configuration
    workspace_path TEXT NOT NULL,
    image TEXT NOT NULL,
    -- Network configuration
    opencode_port INTEGER NOT NULL,
    fileserver_port INTEGER NOT NULL,
    ttyd_port INTEGER NOT NULL,
    -- Status tracking
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'starting', 'running', 'stopping', 'stopped', 'failed')),
    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    stopped_at TEXT,
    -- Error tracking
    error_message TEXT
);

-- Index for finding active sessions
CREATE INDEX IF NOT EXISTS idx_sessions_status ON sessions(status);
CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_container_id ON sessions(container_id);
