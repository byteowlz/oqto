-- Add stable linux username mapping for local multi-user isolation.

ALTER TABLE users ADD COLUMN linux_username TEXT;

-- Backfill existing rows.
UPDATE users SET linux_username = username WHERE linux_username IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_linux_username
ON users(linux_username)
WHERE linux_username IS NOT NULL;
