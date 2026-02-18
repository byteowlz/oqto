-- Add linux_uid column to users table for UID verification
-- This is used to verify Linux user ownership since GECOS can be modified by users via chfn

ALTER TABLE users ADD COLUMN linux_uid INTEGER;

-- Create index for faster lookups by linux_uid
CREATE INDEX IF NOT EXISTS idx_users_linux_uid ON users(linux_uid) WHERE linux_uid IS NOT NULL;
