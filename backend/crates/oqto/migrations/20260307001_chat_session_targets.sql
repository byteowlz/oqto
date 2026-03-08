CREATE TABLE IF NOT EXISTS chat_session_targets (
    session_id TEXT PRIMARY KEY NOT NULL,
    owner_user_id TEXT,
    scope TEXT NOT NULL CHECK (scope IN ('personal', 'shared_workspace')),
    workspace_id TEXT,
    workspace_path TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_chat_session_targets_scope ON chat_session_targets(scope);
CREATE INDEX IF NOT EXISTS idx_chat_session_targets_workspace_id ON chat_session_targets(workspace_id);
CREATE INDEX IF NOT EXISTS idx_chat_session_targets_owner_user_id ON chat_session_targets(owner_user_id);
