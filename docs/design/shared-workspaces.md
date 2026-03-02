# Shared Workspaces Design

Status: DRAFT
Date: 2026-02-27
Related: octo-pdb4, octo-t2bf, workspace-x7gm, octo-6nhg, octo-mj2r

## Overview

Shared workspaces allow multiple users to collaborate on code through a shared filesystem directory with a dedicated runner. Users can jointly send messages to sessions, with their names prepended so the agent understands it is talking to multiple people. A `USERS.md` file is automatically generated and loaded into agent context.

## Core Concepts

### Shared Workspace

A shared workspace is a directory owned by a dedicated Linux user (e.g., `oqto_shared_<name>`) that multiple platform users have access to. It acts as a container for one or more workdirs.

```
/home/oqto_shared_myteam/
  oqto/
    USERS.md                    # Auto-generated, lists members and roles
    .oqto/
      workspace.toml            # Workspace metadata (display_name, shared=true)
      shared.toml               # Shared workspace config (member list, permissions)
    frontend/                 # Individual workdir
      .oqto/workspace.toml
      src/...
    backend/                  # Individual workdir
      .oqto/workspace.toml
      src/...
```

### Workdirs Inside Shared Workspaces

Each workdir within a shared workspace has the same features as regular workdirs:
- Own `.oqto/workspace.toml`
- Own sessions (stored in hstry scoped to the workdir path)
- Own file tree
- Own terminal

### USERS.md

Auto-generated at the shared workspace root. Updated whenever members change. Loaded by Pi as context. Example:

```markdown
# Team Members

This is a shared workspace. Multiple users may send messages in this session.
Messages are prefixed with the sender's name in square brackets.

## Members

| Name | Username | Role |
|------|----------|------|
| Alice Smith | alice | owner |
| Bob Jones | bob | admin |
| Charlie Brown | charlie | member |

## Conventions

- Messages from users appear as: [Alice] Can you refactor the auth module?
- When addressing a specific user's request, mention their name.
- All members can see the full conversation history.
```

### User-Prefixed Messages

When a user sends a prompt in a shared workspace session, the backend prepends their display name:

```
Original:  "Can you refactor the auth module?"
Sent to Pi: "[Alice] Can you refactor the auth module?"
```

This is transparent to the agent -- it sees bracketed names and can address users by name.

## Data Model

### Database Tables

```sql
-- Shared workspaces
CREATE TABLE IF NOT EXISTS shared_workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,                      -- Human-readable name (unique)
    slug TEXT UNIQUE NOT NULL,               -- URL-safe slug (lowercase, hyphens)
    linux_user TEXT NOT NULL,                -- Linux user (oqto_shared_<slug>)
    path TEXT NOT NULL,                      -- Filesystem path (/home/oqto_shared_<slug>/oqto)
    owner_id TEXT NOT NULL REFERENCES users(id),
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Shared workspace members
CREATE TABLE IF NOT EXISTS shared_workspace_members (
    shared_workspace_id TEXT NOT NULL REFERENCES shared_workspaces(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id),
    role TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('owner', 'admin', 'member', 'viewer')),
    added_at TEXT NOT NULL DEFAULT (datetime('now')),
    added_by TEXT REFERENCES users(id),
    PRIMARY KEY (shared_workspace_id, user_id)
);
```

### Roles

| Role | Permissions |
|------|------------|
| **owner** | Full control: manage members, delete workspace, create/delete workdirs, run sessions |
| **admin** | Manage members (except owner), create/delete workdirs, run sessions |
| **member** | Create workdirs, run sessions, send prompts |
| **viewer** | Read-only: view sessions and files, no prompting |

## Architecture

### Linux User Isolation

Each shared workspace gets a dedicated Linux user:
- Username: `oqto_shared_<slug>` (e.g., `oqto_shared_myteam`)
- Home: `/home/oqto_shared_<slug>/` (workspace root at `/home/oqto_shared_<slug>/oqto`)
- Group: `oqto` (shared with all platform users for backend access)
- The dedicated user owns all files in the workspace
- Platform users access files through the backend/runner (not direct filesystem access)

### Runner

Each shared workspace gets its own runner process running as the dedicated Linux user. This runner:
- Owns all agent processes spawned in the workspace
- Has access to the shared filesystem
- Runs hstry scoped to the workspace
- Manages Pi sessions for all workdirs within

The backend routes commands to the shared workspace's runner based on the workspace membership check.

### Session Routing

When a user opens a session in a shared workspace:
1. Backend checks `shared_workspace_members` for access
2. Backend routes to the shared workspace's runner (not the user's personal runner)
3. The runner spawns/manages Pi as the shared Linux user
4. All users with access see the same sessions and history

### Prompt Flow (Multi-User)

```
Frontend: User "Alice" sends "Fix the login bug"
    |
    v
Backend: Check Alice is member of shared workspace
    |
    v
Backend: Prepend user name -> "[Alice] Fix the login bug"
    |
    v
Runner (shared workspace): Forward to Pi session
    |
    v
Pi: Sees "[Alice] Fix the login bug", knows Alice is asking
```

## API Endpoints

### Shared Workspace CRUD

```
POST   /api/shared-workspaces              # Create shared workspace
GET    /api/shared-workspaces              # List workspaces user has access to
GET    /api/shared-workspaces/:id          # Get workspace details
PATCH  /api/shared-workspaces/:id          # Update workspace (name, description)
DELETE /api/shared-workspaces/:id          # Delete workspace (owner only)
```

### Member Management

```
GET    /api/shared-workspaces/:id/members           # List members
POST   /api/shared-workspaces/:id/members           # Add member
PATCH  /api/shared-workspaces/:id/members/:user_id  # Update member role
DELETE /api/shared-workspaces/:id/members/:user_id  # Remove member
```

### Workdir Management (within shared workspace)

```
POST   /api/shared-workspaces/:id/workdirs          # Create workdir
GET    /api/shared-workspaces/:id/workdirs           # List workdirs
DELETE /api/shared-workspaces/:id/workdirs/:name     # Delete workdir
```

## Frontend Integration

### Sidebar

Shared workspaces appear as a separate section in the sidebar, below personal projects:

```
PROJECTS
  my-app
  my-lib

SHARED WORKSPACES
  Team Alpha          [Alice, Bob, Charlie]
    frontend
    backend
  Design System       [Alice, Dave]
    components
```

### Session View

Sessions in shared workspaces show:
- "Shared" badge on the session
- User avatars/names for who's in the session
- Each message shows the sender's name
- Permission-based action buttons (viewers can't prompt)

### Create Dialog

A "New Shared Workspace" dialog accessible from the sidebar:
- Name (required)
- Description (optional)
- Initial members (search/add users)

## Workspace Creation Flows

### 1. Create New Shared Workspace (from scratch)

Available to any user. Creates a fresh shared workspace with no projects.

```
User -> "New Shared Workspace" button in sidebar
     -> SharedWorkspaceDialog: name, description, icon, color
     -> Backend: POST /api/shared-workspaces
        1. Generate slug from name
        2. Create Linux user oqto_shared_<slug> via usermgr
        3. Create DB record (shared_workspaces + owner member)
        4. Generate USERS.md at workspace root
     -> Sidebar refreshes, shows new workspace
```

### 2. Convert Personal Project to Shared Workspace

Available to the project owner. Moves the project directory into a new shared
workspace. The user's existing sessions and history for that path are preserved
(hstry stores by path, so sessions remain accessible once the runner is
re-pointed).

```
User -> Project context menu -> "Share this project..."
     -> ConvertToSharedDialog: workspace name, members to invite
     -> Backend: POST /api/shared-workspaces/convert
        1. Validate user owns the source path
        2. Create Linux user oqto_shared_<slug> via usermgr
        3. Copy or move project files to /home/oqto_shared_<slug>/oqto/<project_name>/
           (using usermgr run-as-user for correct ownership)
        4. Create DB record with user as owner
        5. Add invited members
        6. Generate USERS.md
        7. Optionally leave a symlink or redirect marker at the old path
     -> Sidebar: project disappears from personal, appears under shared
```

The conversion copies files rather than moves them. The user keeps their
personal copy unless they explicitly delete it. This avoids breaking any
local state or git remotes.

### 3. Create Project Inside Shared Workspace

Available to members with `member` role or above. Uses the existing
`POST /api/projects/create-from-template` endpoint but targets the shared
workspace path.

```
User -> Shared workspace context menu -> "New project"
     -> NewProjectDialog (same as personal, but workspace_root = shared path)
     -> Backend: validates user is member, routes to shared workspace runner
```

## Administration Model

### Self-Service (Workspace-Level)

Each shared workspace is self-administered by its owner and admins.

| Action | Owner | Admin | Member | Viewer |
|--------|-------|-------|--------|--------|
| Rename/edit workspace | Yes | Yes | No | No |
| Change icon/color | Yes | Yes | No | No |
| Add members | Yes | Yes | No | No |
| Remove members | Yes | Yes (not owner) | No | No |
| Change member roles | Yes | Yes (not owner) | No | No |
| Delete workspace | Yes | No | No | No |
| Transfer ownership | Yes | No | No | No |
| Create projects inside | Yes | Yes | Yes | No |
| Send prompts | Yes | Yes | Yes | No |
| View sessions/files | Yes | Yes | Yes | Yes |

Ownership transfer: owner can promote an admin to owner and demote themselves
to admin (two-step in a single API call to avoid orphaned workspaces).

### Platform Admin (System-Level)

Platform admins (users with `role = "admin"` in the users table) have
oversight of all shared workspaces regardless of membership:

| Action | Platform Admin |
|--------|---------------|
| List all shared workspaces | Yes |
| View any workspace details + members | Yes |
| Force-delete a workspace | Yes |
| Force-remove a member | Yes |
| Force-transfer ownership | Yes |
| View usage/quota stats | Yes |

Admin API endpoints:

```
GET    /api/admin/shared-workspaces              # List ALL shared workspaces
GET    /api/admin/shared-workspaces/:id          # Full details + members
DELETE /api/admin/shared-workspaces/:id          # Force delete
PATCH  /api/admin/shared-workspaces/:id/owner    # Force transfer ownership
DELETE /api/admin/shared-workspaces/:id/members/:uid  # Force remove member
```

Admin UI: a "Shared Workspaces" tab in the admin panel showing all workspaces
with owner, member count, creation date, disk usage (future).

## Appearance

### Icons

16 curated Lucide icon names, validated server-side:

`users`, `rocket`, `globe`, `code`, `building`, `shield`, `zap`, `layers`,
`hexagon`, `terminal`, `flask-conical`, `palette`, `brain`, `database`,
`network`, `git-branch`

### Colors

12 muted, desaturated tones that complement the dark green-tinted theme:

| Name | Hex | Usage |
|------|-----|-------|
| Primary green | `#3ba77c` | Theme primary, default |
| Sage | `#5b8a72` | |
| Eucalyptus | `#7c9a92` | |
| Slate teal | `#6b8f9c` | |
| Steel blue | `#5c7d8a` | |
| Dusty blue | `#7b8fa6` | |
| Muted violet | `#8b7fa3` | |
| Mauve | `#9c7b8f` | |
| Dusty rose | `#a67c7c` | |
| Warm sand | `#b0926b` | |
| Olive | `#8a9670` | |
| Seafoam | `#6b9080` | |

Auto-assigned deterministically from slug hash. Overridable via create/update API.

## Security Considerations

- Backend always validates membership before routing to shared runner
- Shared workspace Linux user is isolated from personal users
- File operations go through the runner (no direct filesystem access)
- Viewers cannot send prompts or modify files
- Only owners can delete workspaces
- Only owners and admins can manage members
- Display names sanitized before prompt prepending (strip brackets, control chars)
- Platform admins can force-manage any workspace for governance
