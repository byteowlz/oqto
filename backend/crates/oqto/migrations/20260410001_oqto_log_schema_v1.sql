-- oqto-log v1: authoritative turn-DAG history store

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS oqto_log_sessions (
    session_id TEXT PRIMARY KEY,
    platform_id TEXT NOT NULL,
    external_id TEXT,
    user_id TEXT NOT NULL,
    workspace_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_oqto_log_sessions_platform
    ON oqto_log_sessions(platform_id);
CREATE INDEX IF NOT EXISTS idx_oqto_log_sessions_external
    ON oqto_log_sessions(external_id);
CREATE INDEX IF NOT EXISTS idx_oqto_log_sessions_user_workspace
    ON oqto_log_sessions(user_id, workspace_id);

CREATE TABLE IF NOT EXISTS oqto_log_branches (
    branch_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    parent_branch_id TEXT,
    head_turn_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (session_id) REFERENCES oqto_log_sessions(session_id) ON DELETE CASCADE,
    FOREIGN KEY (parent_branch_id) REFERENCES oqto_log_branches(branch_id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_oqto_log_branches_session
    ON oqto_log_branches(session_id);

CREATE TABLE IF NOT EXISTS oqto_log_turns (
    turn_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    branch_id TEXT NOT NULL,
    parent_turn_id TEXT,
    turn_version INTEGER NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'tool')),
    status TEXT NOT NULL CHECK (status IN ('streaming', 'committed', 'failed', 'aborted')),
    source_kind TEXT,
    source_session_id TEXT,
    source_entry_id TEXT,
    source_hash TEXT,
    source_timestamp TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    committed_at TEXT,
    FOREIGN KEY (session_id) REFERENCES oqto_log_sessions(session_id) ON DELETE CASCADE,
    FOREIGN KEY (branch_id) REFERENCES oqto_log_branches(branch_id) ON DELETE CASCADE,
    FOREIGN KEY (parent_turn_id) REFERENCES oqto_log_turns(turn_id) ON DELETE SET NULL,
    UNIQUE (session_id, turn_version)
);

CREATE INDEX IF NOT EXISTS idx_oqto_log_turns_session_branch_version
    ON oqto_log_turns(session_id, branch_id, turn_version);
CREATE INDEX IF NOT EXISTS idx_oqto_log_turns_parent
    ON oqto_log_turns(parent_turn_id);
CREATE INDEX IF NOT EXISTS idx_oqto_log_turns_source_hash
    ON oqto_log_turns(source_hash);
CREATE UNIQUE INDEX IF NOT EXISTS idx_oqto_log_turns_source_entry_unique
    ON oqto_log_turns(source_kind, source_session_id, source_entry_id)
    WHERE source_kind IS NOT NULL AND source_session_id IS NOT NULL AND source_entry_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS oqto_log_messages (
    message_id TEXT PRIMARY KEY,
    turn_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    kind TEXT NOT NULL,
    role TEXT,
    content TEXT,
    json_payload TEXT,
    source_message_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (turn_id) REFERENCES oqto_log_turns(turn_id) ON DELETE CASCADE,
    UNIQUE (turn_id, seq)
);

CREATE INDEX IF NOT EXISTS idx_oqto_log_messages_turn_seq
    ON oqto_log_messages(turn_id, seq);

CREATE TABLE IF NOT EXISTS oqto_log_events (
    event_id TEXT PRIMARY KEY,
    turn_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    payload TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (turn_id) REFERENCES oqto_log_turns(turn_id) ON DELETE CASCADE,
    UNIQUE (turn_id, seq)
);

CREATE INDEX IF NOT EXISTS idx_oqto_log_events_turn_seq
    ON oqto_log_events(turn_id, seq);

CREATE TABLE IF NOT EXISTS oqto_log_import_checkpoints (
    checkpoint_id TEXT PRIMARY KEY,
    source_kind TEXT NOT NULL,
    source_session_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    last_offset INTEGER,
    last_source_entry_id TEXT,
    last_source_hash TEXT,
    schema_version INTEGER NOT NULL DEFAULT 1,
    last_run_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (source_kind, source_session_id),
    FOREIGN KEY (session_id) REFERENCES oqto_log_sessions(session_id) ON DELETE CASCADE
);

CREATE VIRTUAL TABLE IF NOT EXISTS oqto_log_message_fts USING fts5(
    message_id UNINDEXED,
    turn_id UNINDEXED,
    session_id UNINDEXED,
    role,
    content,
    tokenize = 'unicode61'
);

CREATE TRIGGER IF NOT EXISTS oqto_log_messages_ai
AFTER INSERT ON oqto_log_messages
WHEN NEW.content IS NOT NULL
BEGIN
    INSERT INTO oqto_log_message_fts (rowid, message_id, turn_id, session_id, role, content)
    VALUES (
        NEW.rowid,
        NEW.message_id,
        NEW.turn_id,
        (SELECT session_id FROM oqto_log_turns WHERE turn_id = NEW.turn_id),
        COALESCE(NEW.role, ''),
        NEW.content
    );
END;

CREATE TRIGGER IF NOT EXISTS oqto_log_messages_ad
AFTER DELETE ON oqto_log_messages
BEGIN
    INSERT INTO oqto_log_message_fts (oqto_log_message_fts, rowid, message_id, turn_id, session_id, role, content)
    VALUES ('delete', OLD.rowid, OLD.message_id, OLD.turn_id, '', COALESCE(OLD.role, ''), COALESCE(OLD.content, ''));
END;

CREATE TRIGGER IF NOT EXISTS oqto_log_messages_au
AFTER UPDATE ON oqto_log_messages
BEGIN
    INSERT INTO oqto_log_message_fts (oqto_log_message_fts, rowid, message_id, turn_id, session_id, role, content)
    VALUES ('delete', OLD.rowid, OLD.message_id, OLD.turn_id, '', COALESCE(OLD.role, ''), COALESCE(OLD.content, ''));

    INSERT INTO oqto_log_message_fts (rowid, message_id, turn_id, session_id, role, content)
    VALUES (
        NEW.rowid,
        NEW.message_id,
        NEW.turn_id,
        (SELECT session_id FROM oqto_log_turns WHERE turn_id = NEW.turn_id),
        COALESCE(NEW.role, ''),
        COALESCE(NEW.content, '')
    );
END;
