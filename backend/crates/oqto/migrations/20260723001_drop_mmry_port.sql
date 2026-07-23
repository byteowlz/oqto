-- Remove the mmry service port columns. Memory is embedded mmry-core over each
-- workspace's `.mmry/mmry.jsonl` (ADR-0010); there is no mmry daemon or port.
--
-- Indexes must be dropped before the columns they reference.
DROP INDEX IF EXISTS idx_sessions_active_mmry_port;
DROP INDEX IF EXISTS idx_users_mmry_port;

ALTER TABLE sessions DROP COLUMN mmry_port;
ALTER TABLE users DROP COLUMN mmry_port;
