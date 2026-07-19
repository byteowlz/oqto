-- Durable provenance for hard-copy forks.
--
-- A Fork creates a new independent Session. This lineage is deliberately on
-- the child session, not oqto_log_branches (which models in-session paths).
ALTER TABLE oqto_log_sessions
    ADD COLUMN parent_session_id TEXT REFERENCES oqto_log_sessions(session_id) ON DELETE SET NULL;

ALTER TABLE oqto_log_sessions
    ADD COLUMN forked_from_entry_id TEXT;

ALTER TABLE oqto_log_sessions
    ADD COLUMN forked_at TEXT;

ALTER TABLE oqto_log_sessions
    ADD COLUMN fork_operation_id TEXT;

ALTER TABLE oqto_log_sessions
    ADD COLUMN fork_prompt_text TEXT;

CREATE INDEX IF NOT EXISTS idx_oqto_log_sessions_parent
    ON oqto_log_sessions(parent_session_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_oqto_log_sessions_fork_operation
    ON oqto_log_sessions(fork_operation_id)
    WHERE fork_operation_id IS NOT NULL;
