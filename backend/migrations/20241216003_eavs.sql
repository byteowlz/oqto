-- Add EAVS (LLM proxy) fields to sessions

-- EAVS port for each session
ALTER TABLE sessions ADD COLUMN eavs_port INTEGER;

-- EAVS virtual key ID (human-readable, e.g., "cold-lamp")
ALTER TABLE sessions ADD COLUMN eavs_key_id TEXT;

-- EAVS virtual key hash (for API lookups)
ALTER TABLE sessions ADD COLUMN eavs_key_hash TEXT;

-- The actual virtual key value (stored encrypted or hashed in production)
-- Only set when session is created, cleared after container receives it
ALTER TABLE sessions ADD COLUMN eavs_virtual_key TEXT;

-- Index for EAVS key lookups
CREATE INDEX IF NOT EXISTS idx_sessions_eavs_key_id ON sessions(eavs_key_id);
