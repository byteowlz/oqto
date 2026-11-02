-- Per-invocation owner consent. Pairing itself never grants execution.
CREATE TABLE personal_hands (
    id TEXT PRIMARY KEY NOT NULL,
    device_id TEXT NOT NULL REFERENCES personal_devices(id),
    request_key TEXT NOT NULL,
    session_label TEXT NOT NULL,
    command TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('pending','approved','running','done','denied','cancelled','uncertain')),
    root_json TEXT,
    expires_at INTEGER NOT NULL,
    output TEXT,
    exit_code INTEGER,
    UNIQUE(device_id, request_key)
);
