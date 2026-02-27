-- Add icon and color fields to shared workspaces for visual distinction.
-- icon: lucide icon name (e.g., "users", "rocket", "globe")
-- color: hex color for accent (e.g., "#6366f1")

ALTER TABLE shared_workspaces ADD COLUMN icon TEXT NOT NULL DEFAULT 'users';
ALTER TABLE shared_workspaces ADD COLUMN color TEXT NOT NULL DEFAULT '#6366f1';
