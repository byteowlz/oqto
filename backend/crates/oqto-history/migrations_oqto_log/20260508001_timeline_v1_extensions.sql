-- oqto-log timeline v1 extensions: lossless raw envelopes, context snapshots, and richer graph metadata

PRAGMA foreign_keys = ON;

ALTER TABLE oqto_log_sessions ADD COLUMN title TEXT;
ALTER TABLE oqto_log_sessions ADD COLUMN schema_version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE oqto_log_sessions ADD COLUMN extensions_json TEXT;

ALTER TABLE oqto_log_branches ADD COLUMN forked_from_turn_id TEXT REFERENCES oqto_log_turns(turn_id) ON DELETE SET NULL;
ALTER TABLE oqto_log_branches ADD COLUMN extensions_json TEXT;

ALTER TABLE oqto_log_turns ADD COLUMN stop_reason TEXT;
ALTER TABLE oqto_log_turns ADD COLUMN usage_json TEXT;
ALTER TABLE oqto_log_turns ADD COLUMN context_snapshot_id TEXT;
ALTER TABLE oqto_log_turns ADD COLUMN extensions_json TEXT;

ALTER TABLE oqto_log_messages ADD COLUMN seq_in_turn INTEGER;
ALTER TABLE oqto_log_messages ADD COLUMN completed_at TEXT;
ALTER TABLE oqto_log_messages ADD COLUMN parts_json TEXT;
ALTER TABLE oqto_log_messages ADD COLUMN raw_refs_json TEXT;
ALTER TABLE oqto_log_messages ADD COLUMN extensions_json TEXT;

CREATE TABLE IF NOT EXISTS oqto_log_parts (
    part_id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('content', 'tool_call', 'tool_result', 'delegation')),
    tool_call_id TEXT,
    tool_name TEXT,
    title TEXT,
    status TEXT,
    is_error INTEGER,
    json_payload TEXT,
    raw_refs_json TEXT,
    started_at TEXT,
    completed_at TEXT,
    extensions_json TEXT,
    FOREIGN KEY (message_id) REFERENCES oqto_log_messages(message_id) ON DELETE CASCADE,
    FOREIGN KEY (turn_id) REFERENCES oqto_log_turns(turn_id) ON DELETE CASCADE,
    UNIQUE (message_id, seq)
);

CREATE INDEX IF NOT EXISTS idx_oqto_log_parts_turn_seq
    ON oqto_log_parts(turn_id, seq);
CREATE INDEX IF NOT EXISTS idx_oqto_log_parts_tool_call
    ON oqto_log_parts(tool_call_id)
    WHERE tool_call_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS oqto_log_raw_envelopes (
    raw_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    turn_id TEXT,
    source TEXT NOT NULL,
    harness TEXT NOT NULL,
    native_type TEXT NOT NULL,
    source_sequence INTEGER NOT NULL,
    received_at TEXT NOT NULL,
    native_schema_version TEXT,
    payload_json TEXT NOT NULL,
    payload_sha256 TEXT,
    extensions_json TEXT,
    FOREIGN KEY (session_id) REFERENCES oqto_log_sessions(session_id) ON DELETE CASCADE,
    FOREIGN KEY (turn_id) REFERENCES oqto_log_turns(turn_id) ON DELETE SET NULL,
    UNIQUE (session_id, source, harness, source_sequence)
);

CREATE INDEX IF NOT EXISTS idx_oqto_log_raw_envelopes_session_seq
    ON oqto_log_raw_envelopes(session_id, source_sequence);
CREATE INDEX IF NOT EXISTS idx_oqto_log_raw_envelopes_payload_hash
    ON oqto_log_raw_envelopes(payload_sha256)
    WHERE payload_sha256 IS NOT NULL;

CREATE TABLE IF NOT EXISTS oqto_log_context_snapshots (
    snapshot_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    turn_id TEXT,
    captured_at TEXT NOT NULL,
    platform TEXT NOT NULL,
    harness TEXT NOT NULL,
    workspace_id TEXT,
    user_id TEXT NOT NULL,
    request_id TEXT,
    correlation_id TEXT,
    sandbox_json TEXT,
    model TEXT,
    provider TEXT,
    readable_id TEXT,
    context_source TEXT,
    system_prompt_hash TEXT,
    included_turn_ids_json TEXT,
    included_file_refs_json TEXT,
    usage_json TEXT,
    extensions_json TEXT,
    FOREIGN KEY (session_id) REFERENCES oqto_log_sessions(session_id) ON DELETE CASCADE,
    FOREIGN KEY (turn_id) REFERENCES oqto_log_turns(turn_id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_oqto_log_context_snapshots_session
    ON oqto_log_context_snapshots(session_id, captured_at);

CREATE TABLE IF NOT EXISTS oqto_log_hstry_projection_checkpoints (
    checkpoint_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    hstry_conversation_id TEXT,
    last_turn_version INTEGER,
    last_projected_hash TEXT,
    projected_at TEXT NOT NULL DEFAULT (datetime('now')),
    status TEXT NOT NULL CHECK (status IN ('ok', 'failed')),
    error TEXT,
    FOREIGN KEY (session_id) REFERENCES oqto_log_sessions(session_id) ON DELETE CASCADE,
    UNIQUE (session_id)
);
