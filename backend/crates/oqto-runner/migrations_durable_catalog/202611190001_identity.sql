-- Runner-owned identity only. No transcripts, credentials or permission decisions.
CREATE TABLE durable_stores (
    store_id TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    UNIQUE (workspace_id, agent_id)
);

CREATE TABLE durable_sessions (
    public_session_id TEXT PRIMARY KEY NOT NULL,
    store_id TEXT NOT NULL REFERENCES durable_stores(store_id),
    native_conversation_id TEXT NOT NULL,
    origin_work_directory_id TEXT NOT NULL,
    UNIQUE (store_id, native_conversation_id)
);
