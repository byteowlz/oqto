-- Authoritative private preferences for one Account using one App Instance.
-- App source, shared domain data, and credentials never belong here.
CREATE TABLE IF NOT EXISTS app_instance_kv (
    instance_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY(instance_id, account_id, key),
    FOREIGN KEY(instance_id) REFERENCES app_instances(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_app_instance_kv_account
    ON app_instance_kv(account_id, instance_id, key);
