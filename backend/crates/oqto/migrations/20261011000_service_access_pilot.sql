-- Bounded external service access, not App action grants or Operator API roles.
CREATE TABLE service_access_grants (
    id TEXT PRIMARY KEY NOT NULL,
    workload_id TEXT NOT NULL CHECK (workload_id = 'design-review'),
    service_id TEXT NOT NULL CHECK (service_id = 'design-studio'),
    workspace_id TEXT NOT NULL REFERENCES shared_workspaces(id) ON DELETE CASCADE,
    resource_id TEXT NOT NULL CHECK (resource_id = 'catalog'),
    audience TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    revoked_at INTEGER,
    credential_hash TEXT UNIQUE NOT NULL,
    issued_by TEXT NOT NULL REFERENCES users(id),
    created_at INTEGER NOT NULL
);
CREATE TABLE service_access_audit (
    event_id TEXT PRIMARY KEY NOT NULL,
    occurred_at INTEGER NOT NULL,
    operation TEXT NOT NULL,
    actor_id TEXT,
    grant_id TEXT,
    decision TEXT NOT NULL,
    reason TEXT NOT NULL
);
