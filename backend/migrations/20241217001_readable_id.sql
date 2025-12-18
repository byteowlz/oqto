-- Add readable_id column to sessions table
-- Format: adjective-noun (e.g., "cold-lamp", "blue-frog")
-- This provides a human-friendly identifier for sessions

ALTER TABLE sessions ADD COLUMN readable_id TEXT;

-- Create unique index on readable_id for fast lookups
CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_readable_id ON sessions(readable_id);
