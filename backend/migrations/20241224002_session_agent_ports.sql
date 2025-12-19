-- Add agent port range to sessions
-- Each session reserves a range of ports for sub-agents

-- Base port for sub-agents (external). Sub-agents use ports agent_base_port to agent_base_port + max_agents - 1
ALTER TABLE sessions ADD COLUMN agent_base_port INTEGER;

-- Maximum number of sub-agents allowed for this session
ALTER TABLE sessions ADD COLUMN max_agents INTEGER DEFAULT 10;
