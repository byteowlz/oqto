-- Workspace locations for shared/multi-runner setups.
CREATE TABLE IF NOT EXISTS workspace_locations (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    location_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('local', 'remote')),
    path TEXT NOT NULL,
    runner_id TEXT,
    repo_fingerprint TEXT,
    is_active INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(user_id, workspace_id, location_id)
);

CREATE INDEX IF NOT EXISTS idx_workspace_locations_user_workspace
    ON workspace_locations(user_id, workspace_id);
CREATE INDEX IF NOT EXISTS idx_workspace_locations_active
    ON workspace_locations(user_id, workspace_id, is_active);
