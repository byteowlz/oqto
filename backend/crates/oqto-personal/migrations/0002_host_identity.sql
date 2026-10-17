CREATE TABLE personal_host_identity (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    public_id TEXT NOT NULL UNIQUE
);
