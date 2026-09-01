-- Oqto App lifecycle v0 (ADR-0038).
--
-- Paths are private binding/provenance facts. Public contracts use opaque ids.
-- Definitions and Instances remain pinned; Installation updates never rewrite
-- an existing Instance's definition.

CREATE TABLE IF NOT EXISTS work_directories (
    id TEXT PRIMARY KEY NOT NULL,
    owner_kind TEXT NOT NULL CHECK (owner_kind IN ('account', 'workspace')),
    owner_id TEXT NOT NULL,
    binding_path TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(owner_kind, owner_id, binding_path)
);

CREATE TABLE IF NOT EXISTS app_definitions (
    id TEXT PRIMARY KEY NOT NULL,
    content_digest TEXT NOT NULL UNIQUE,
    app_id TEXT NOT NULL,
    version TEXT NOT NULL,
    title_json TEXT NOT NULL,
    description TEXT,
    schema_tag TEXT NOT NULL,
    manifest_toml TEXT NOT NULL,
    web_entry_path TEXT NOT NULL,
    file_index_json TEXT NOT NULL,
    -- Canonical validated capability request, pinned with the Definition.
    -- A grant is written from this exact JSON, never re-derived from mutable
    -- source, so approving a Definition can never approve different authority.
    requested_capabilities_json TEXT NOT NULL DEFAULT '[]',
    total_bytes INTEGER NOT NULL CHECK (total_bytes >= 0),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS app_installations (
    id TEXT PRIMARY KEY NOT NULL,
    owner_kind TEXT NOT NULL CHECK (
        owner_kind IN ('work_directory', 'workspace', 'account', 'deployment')
    ),
    owner_id TEXT NOT NULL,
    app_id TEXT NOT NULL,
    current_definition_id TEXT NOT NULL,
    source_work_directory_id TEXT,
    source_package_name TEXT,
    provenance_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(owner_kind, owner_id, app_id),
    FOREIGN KEY(current_definition_id) REFERENCES app_definitions(id),
    FOREIGN KEY(source_work_directory_id) REFERENCES work_directories(id)
);

CREATE TABLE IF NOT EXISTS app_instances (
    id TEXT PRIMARY KEY NOT NULL,
    installation_id TEXT NOT NULL,
    definition_id TEXT NOT NULL,
    binding_kind TEXT NOT NULL CHECK (
        binding_kind IN ('work_directory', 'workspace', 'account', 'deployment')
    ),
    binding_id TEXT NOT NULL,
    -- A capability-requesting Instance is born 'awaiting_permission': it is
    -- pinned and addressable but holds no authority and cannot present.
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

CREATE TABLE IF NOT EXISTS app_publish_workflows (
    id TEXT PRIMARY KEY NOT NULL,
    acting_account_id TEXT NOT NULL,
    work_directory_id TEXT NOT NULL,
    app_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (
        status IN (
            'validating',
            'materializing',
            'awaiting_permission',
            'approved',
            'denied',
            'instance_ready',
            'failed'
        )
    ),
    definition_id TEXT,
    installation_id TEXT,
    instance_id TEXT,
    rejection_code TEXT,
    rejection_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY(work_directory_id) REFERENCES work_directories(id),
    FOREIGN KEY(definition_id) REFERENCES app_definitions(id),
    FOREIGN KEY(installation_id) REFERENCES app_installations(id),
    FOREIGN KEY(instance_id) REFERENCES app_instances(id)
);


-- One current permission decision per Instance, bound to the exact Definition
-- and digest the deciding Account reviewed. Republishing changed source creates
-- a different Definition and therefore a different Instance, which starts
-- undecided rather than inheriting this row.
CREATE TABLE IF NOT EXISTS app_capability_grants (
    id TEXT PRIMARY KEY NOT NULL,
    instance_id TEXT NOT NULL UNIQUE,
    definition_id TEXT NOT NULL,
    content_digest TEXT NOT NULL,
    -- Exact canonical request that was approved or refused.
    request_json TEXT NOT NULL,
    decision TEXT NOT NULL CHECK (decision IN ('allowed', 'denied')),
    decided_by_account_id TEXT NOT NULL,
    decided_at TEXT NOT NULL DEFAULT (datetime('now')),
    revoked_at TEXT,
    revoked_by_account_id TEXT,
    FOREIGN KEY(instance_id) REFERENCES app_instances(id),
    FOREIGN KEY(definition_id) REFERENCES app_definitions(id)
);

CREATE INDEX IF NOT EXISTS idx_app_capability_grants_active
    ON app_capability_grants(instance_id, decision, revoked_at);

CREATE INDEX IF NOT EXISTS idx_app_installations_owner
    ON app_installations(owner_kind, owner_id);
CREATE INDEX IF NOT EXISTS idx_app_instances_binding
    ON app_instances(binding_kind, binding_id, status);
CREATE INDEX IF NOT EXISTS idx_app_publish_workflows_workdir
    ON app_publish_workflows(work_directory_id, created_at);
