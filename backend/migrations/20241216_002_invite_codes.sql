-- Invite codes table for self-service registration
-- Users must have a valid invite code to register

CREATE TABLE IF NOT EXISTS invite_codes (
    id TEXT PRIMARY KEY NOT NULL,
    -- The invite code itself (what users enter)
    code TEXT UNIQUE NOT NULL,
    -- Who created this invite code (admin user_id)
    created_by TEXT NOT NULL REFERENCES users(id),
    -- Optional: limit who can use this code (NULL = anyone)
    -- For multi-use codes, this tracks the last user who used it
    used_by TEXT REFERENCES users(id),
    -- How many times this code can still be used (0 = exhausted)
    -- Set to 1 for single-use codes, higher for multi-use
    uses_remaining INTEGER NOT NULL DEFAULT 1,
    -- Maximum total uses (for tracking purposes)
    max_uses INTEGER NOT NULL DEFAULT 1,
    -- Optional expiration date
    expires_at TEXT,
    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    -- When the code was last used
    last_used_at TEXT,
    -- Optional note/label for admin tracking
    note TEXT
);

-- Index for fast code lookup during registration
CREATE INDEX IF NOT EXISTS idx_invite_codes_code ON invite_codes(code);

-- Index for listing codes by creator
CREATE INDEX IF NOT EXISTS idx_invite_codes_created_by ON invite_codes(created_by);

-- Index for finding expired codes
CREATE INDEX IF NOT EXISTS idx_invite_codes_expires_at ON invite_codes(expires_at);

-- Index for finding exhausted codes
CREATE INDEX IF NOT EXISTS idx_invite_codes_uses_remaining ON invite_codes(uses_remaining);
