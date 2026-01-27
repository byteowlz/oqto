-- Add sldr_port column for per-user sldr-server instances
ALTER TABLE users ADD COLUMN sldr_port INTEGER;

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_sldr_port
ON users(sldr_port)
WHERE sldr_port IS NOT NULL;
