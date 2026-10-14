-- Host actions are canonical oqto-log records, not invented chat Sessions.
CREATE TABLE host_action_audit (
    event_id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL,
    action TEXT NOT NULL,
    connection_id TEXT NOT NULL,
    connection_revision INTEGER,
    instance_id TEXT,
    slot TEXT,
    content_digest TEXT,
    recorded_at_ms INTEGER NOT NULL
);
CREATE INDEX host_action_audit_account ON host_action_audit(account_id, recorded_at_ms);
