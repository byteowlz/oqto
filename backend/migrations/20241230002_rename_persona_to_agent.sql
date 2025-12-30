-- Rename persona_path column to agent
-- The agent field now stores the opencode agent name (passed via --agent flag)
-- rather than a filesystem path to a persona directory.

-- SQLite doesn't support RENAME COLUMN directly in older versions,
-- but the version we use does. If this fails, we'd need to recreate the table.
ALTER TABLE sessions RENAME COLUMN persona_path TO agent;
