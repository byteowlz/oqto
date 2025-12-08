-- Users table: stores user accounts for the platform
-- Supports both local authentication (dev mode) and OIDC (production)

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY NOT NULL,
    -- Authentication
    external_id TEXT UNIQUE,          -- OIDC subject claim (sub) for external auth
    username TEXT UNIQUE NOT NULL,
    email TEXT UNIQUE NOT NULL,
    password_hash TEXT,               -- Only used in dev mode, NULL for OIDC users
    -- Profile
    display_name TEXT NOT NULL,
    avatar_url TEXT,
    -- Authorization
    role TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'admin', 'service')),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    -- Metadata
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_login_at TEXT,
    -- Settings (JSON blob for extensibility)
    settings TEXT DEFAULT '{}'
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_external_id ON users(external_id);
CREATE INDEX IF NOT EXISTS idx_users_role ON users(role);
CREATE INDEX IF NOT EXISTS idx_users_is_active ON users(is_active);

-- Update the sessions table to reference users properly
-- Note: SQLite doesn't support ALTER TABLE ADD CONSTRAINT, so we document the relationship
-- The user_id column in sessions should reference users(id)

-- Insert default admin user (for dev mode)
INSERT OR IGNORE INTO users (id, username, email, display_name, role, password_hash)
VALUES (
    'usr_admin',
    'admin',
    'admin@localhost',
    'Administrator',
    'admin',
    -- bcrypt hash of 'admin' with cost 4 (for dev speed)
    '$2y$04$ynG2vPKhOB4JEz7zekBVWOS3.dzY2fE11lsSpoPGiGrhIv4uAl.PS'
);

-- Insert default regular user (for dev mode)  
INSERT OR IGNORE INTO users (id, username, email, display_name, role, password_hash)
VALUES (
    'usr_dev',
    'dev',
    'dev@localhost',
    'Developer',
    'user',
    -- bcrypt hash of 'dev' with cost 4
    '$2y$04$UDO0uMncBrVNxHtRT1MIUuAHw.JYKkP57W6geyrYfP747xTIUl06u'
);
