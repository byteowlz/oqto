-- Sessions may run on a machine Oqto does not own. SQLite cannot alter a CHECK
-- constraint in place, so the table is rebuilt with the existing rows preserved.
CREATE TABLE chat_session_targets_new (
    session_id TEXT PRIMARY KEY NOT NULL,
    owner_user_id TEXT,
    scope TEXT NOT NULL CHECK (scope IN ('personal', 'shared_workspace', 'remote_machine')),
    workspace_id TEXT,
    workspace_path TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO chat_session_targets_new (
    session_id, owner_user_id, scope, workspace_id, workspace_path, updated_at
)
SELECT session_id, owner_user_id, scope, workspace_id, workspace_path, updated_at
FROM chat_session_targets;

DROP TABLE chat_session_targets;
ALTER TABLE chat_session_targets_new RENAME TO chat_session_targets;

CREATE INDEX IF NOT EXISTS idx_chat_session_targets_scope ON chat_session_targets(scope);
CREATE INDEX IF NOT EXISTS idx_chat_session_targets_workspace_id ON chat_session_targets(workspace_id);
CREATE INDEX IF NOT EXISTS idx_chat_session_targets_owner_user_id ON chat_session_targets(owner_user_id);
