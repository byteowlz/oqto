PRAGMA application_id = 0x4f515050;

CREATE TABLE personal_clock (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    watermark INTEGER NOT NULL
);
INSERT INTO personal_clock (id, watermark) VALUES (1, 0);

CREATE TABLE personal_pairing_tickets (
    token_hash TEXT PRIMARY KEY,
    grants_json TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    consumed_at INTEGER
);

CREATE TABLE personal_devices (
    id TEXT PRIMARY KEY,
    public_key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    grants_json TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    revoked_at INTEGER
);

CREATE TABLE personal_request_nonces (
    device_id TEXT NOT NULL REFERENCES personal_devices(id),
    nonce TEXT NOT NULL,
    accepted_at INTEGER NOT NULL,
    PRIMARY KEY (device_id, nonce)
);
