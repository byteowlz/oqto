CREATE TABLE posts (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    post_id TEXT NOT NULL UNIQUE,
    identity TEXT NOT NULL,
    created INTEGER NOT NULL,
    payload TEXT NOT NULL
);
CREATE TABLE receipts (
    identity TEXT NOT NULL,
    request_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    created INTEGER NOT NULL,
    result TEXT NOT NULL,
    PRIMARY KEY (identity, request_id)
);
CREATE INDEX receipts_rate ON receipts(identity, operation, created);
CREATE TABLE reply_outbox (
    identity TEXT NOT NULL,
    request_id TEXT NOT NULL,
    target TEXT NOT NULL,
    reply TEXT NOT NULL,
    delivered INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (identity, request_id),
    FOREIGN KEY (identity, request_id) REFERENCES receipts(identity, request_id)
);
CREATE TABLE store_meta (key TEXT PRIMARY KEY, value INTEGER NOT NULL);
INSERT INTO store_meta VALUES ('pruned_through', 0);
