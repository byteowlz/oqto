-- Evolve the immutable v0 App schema without modifying migration 20260901001.
ALTER TABLE app_definitions
    ADD COLUMN requested_capabilities_json TEXT NOT NULL DEFAULT '[]';

-- SQLite cannot extend a CHECK constraint in place. Build the replacement
-- under a temporary name so inbound foreign keys continue to name
-- app_instances after the final rename.
PRAGMA defer_foreign_keys = ON;
CREATE TABLE app_instances_v1 (
    id TEXT PRIMARY KEY NOT NULL,
    installation_id TEXT NOT NULL,
    definition_id TEXT NOT NULL,
    binding_kind TEXT NOT NULL CHECK (
        binding_kind IN ('work_directory', 'workspace', 'account', 'deployment')
    ),
    binding_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (
        status IN ('active', 'awaiting_permission', 'suspended', 'unavailable')
    ),
    created_by_account_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(installation_id, definition_id, binding_kind, binding_id),
    FOREIGN KEY(installation_id) REFERENCES app_installations(id),
    FOREIGN KEY(definition_id) REFERENCES app_definitions(id)
);
INSERT INTO app_instances_v1 (
    id, installation_id, definition_id, binding_kind, binding_id, status,
    created_by_account_id, created_at, updated_at
)
SELECT
    id, installation_id, definition_id, binding_kind, binding_id, status,
    created_by_account_id, created_at, updated_at
FROM app_instances;
DROP TABLE app_instances;
ALTER TABLE app_instances_v1 RENAME TO app_instances;

CREATE TABLE app_capability_grants (
    id TEXT PRIMARY KEY NOT NULL,
    instance_id TEXT NOT NULL UNIQUE,
    definition_id TEXT NOT NULL,
    content_digest TEXT NOT NULL,
    request_json TEXT NOT NULL,
    decision TEXT NOT NULL CHECK (decision IN ('allowed', 'denied')),
    decided_by_account_id TEXT NOT NULL,
    decided_at TEXT NOT NULL DEFAULT (datetime('now')),
    revoked_at TEXT,
    revoked_by_account_id TEXT,
    FOREIGN KEY(instance_id) REFERENCES app_instances(id),
    FOREIGN KEY(definition_id) REFERENCES app_definitions(id)
);

CREATE INDEX idx_app_capability_grants_active
    ON app_capability_grants(instance_id, decision, revoked_at);
CREATE INDEX idx_app_instances_binding
    ON app_instances(binding_kind, binding_id, status);
