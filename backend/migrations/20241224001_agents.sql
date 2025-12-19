-- Agents table: tracks opencode agent instances within sessions
-- Each agent runs on a specific port inside the container

CREATE TABLE IF NOT EXISTS agents (
    -- Unique agent ID (session_id + agent_id combination)
    id TEXT PRIMARY KEY NOT NULL,
    -- Session this agent belongs to
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    -- Agent identifier (directory name, e.g., "doc-writer" or "main" for workspace root)
    agent_id TEXT NOT NULL,
    -- Human-readable name
    name TEXT NOT NULL,
    -- Directory path inside the container
    directory TEXT NOT NULL,
    -- Internal port (inside container, e.g., 4001)
    internal_port INTEGER NOT NULL,
    -- External port (mapped to host, e.g., 41824)
    external_port INTEGER NOT NULL,
    -- Current status: running, starting, stopped, failed
    status TEXT NOT NULL DEFAULT 'stopped' CHECK (status IN ('running', 'starting', 'stopped', 'failed')),
    -- Whether the directory has an AGENTS.md file
    has_agents_md INTEGER NOT NULL DEFAULT 0,
    -- Whether the directory is a git repository
    has_git INTEGER NOT NULL DEFAULT 0,
    -- When the agent was created
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    -- When the agent was last started
    started_at TEXT,
    -- When the agent was last stopped
    stopped_at TEXT,
    -- Unique constraint on session + agent_id
    UNIQUE(session_id, agent_id)
);

-- Index for finding agents by session
CREATE INDEX IF NOT EXISTS idx_agents_session_id ON agents(session_id);

-- Index for finding running agents
CREATE INDEX IF NOT EXISTS idx_agents_status ON agents(status);

-- Partial unique index to prevent port conflicts for running agents
CREATE UNIQUE INDEX IF NOT EXISTS idx_agents_external_port_active 
ON agents(external_port) 
WHERE status IN ('running', 'starting');
