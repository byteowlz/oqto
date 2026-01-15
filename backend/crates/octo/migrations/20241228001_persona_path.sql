-- Add persona_path column to sessions table
-- When set, fileserver serves files from this directory instead of workspace_path.
-- This is typically used to show only persona-specific files (AGENTS.md, .beads/, etc.)
-- while keeping the full workspace available for the terminal/opencode.

ALTER TABLE sessions ADD COLUMN persona_path TEXT;
