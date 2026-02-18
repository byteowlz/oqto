-- Per-user mmry port allocation for local multi-user mode.
--
-- Each user gets a stable port (dedicated range) so that per-user mmry instances
-- can be reached reliably across restarts.

ALTER TABLE users ADD COLUMN mmry_port INTEGER;

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_mmry_port
ON users(mmry_port)
WHERE mmry_port IS NOT NULL;
