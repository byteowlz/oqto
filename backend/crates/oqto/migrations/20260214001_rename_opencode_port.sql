-- Rename opencode_port to agent_port.
-- OpenCode harness has been removed; this port is now available for
-- the Pi agent HTTP bridge or future agent runtimes.
ALTER TABLE sessions RENAME COLUMN opencode_port TO agent_port;

-- Recreate the unique index with the new column name.
-- SQLite keeps the old index pointing at the renamed column, but the
-- index name still says "opencode".  Drop + recreate for clarity.
DROP INDEX IF EXISTS idx_sessions_active_opencode_port;
CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_active_agent_port
ON sessions(agent_port)
WHERE status IN ('pending', 'starting', 'running');
