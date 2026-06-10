# Oqto API Reference

## Table of Contents

1. [Authentication](#authentication)
2. [Sessions](#sessions)
3. [Chat History](#chat-history)
4. [Projects and Workspaces](#projects-and-workspaces)
5. [WebSocket](#websocket)
6. [Voice](#voice)
7. [Memories (mmry)](#memories-mmry)
8. [Settings](#settings)
9. [Agent Browser](#agent-browser)
10. [A2UI (Agent-to-UI)](#a2ui-agent-to-ui)
11. [Delegation](#delegation)
12. [Onboarding](#onboarding)
13. [Scheduler (sldr)](#scheduler-sldr)
14. [UI Control](#ui-control)
15. [User Management](#user-management)
16. [Admin Routes](#admin-routes)
17. [Miscellaneous](#miscellaneous)

---

## Authentication

### POST /api/auth/login
Login with email and password. Sets JWT cookie.

### POST /api/auth/register
Register with invite code.

### POST /api/auth/logout
Clear authentication cookie.

### POST /api/auth/dev-login
Dev mode login (only when `auth.dev_mode = true`).

### POST /api/auth/change-password
Change current user's password (authenticated).

---

## Sessions

### GET /api/sessions
List all sessions for current user.

### POST /api/sessions
Create a new session.

### POST /api/sessions/get-or-create
Get an existing session or create one (by project path or workspace).

### POST /api/sessions/get-or-create-for-workspace
Get or create session for a specific workspace.

### GET /api/sessions/{session_id}
Get session details.

### DELETE /api/sessions/{session_id}
Delete a session.

### POST /api/sessions/{session_id}/stop
Stop a running session.

### POST /api/sessions/{session_id}/resume
Resume a stopped session.

### POST /api/sessions/{session_id}/activity
Touch session activity timestamp (keeps session alive).

### GET /api/sessions/{session_id}/update
Check if updates are available for a session.

### POST /api/sessions/{session_id}/upgrade
Upgrade session to latest container image.

### GET /api/sessions/updates
Check if updates are available for any sessions.

---

## Chat History

All reads from hstry (gRPC) or Pi session files on disk.

### GET /api/chat-history
List all chat sessions.

### GET /api/chat-history/grouped
List chat sessions grouped by time (today, yesterday, last week, etc.).

### GET /api/chat-history/{session_id}
Get a specific chat session's metadata.

### PATCH /api/chat-history/{session_id}
Update chat session (e.g., rename title).

### GET /api/chat-history/{session_id}/messages
Get all messages for a chat session.

### GET /api/search
Search across sessions (full-text search via hstry).

---

## Projects and Workspaces

### GET /api/projects
List workspace directories.

### GET /api/projects/logo/{*path}
Get project logo image.

### GET /api/projects/locations
List workspace locations (roots).

### POST /api/projects/locations
Add or update a workspace location.

### POST /api/projects/locations/active
Set the active workspace location.

### GET /api/projects/templates
List available project templates.

### POST /api/projects/templates
Create a new project from a template (uses scaffold system).

### GET /api/workspace/meta
Get workspace metadata.

### PATCH /api/workspace/meta
Update workspace metadata.

### GET /api/workspace/sandbox
Get workspace sandbox configuration.

### PATCH /api/workspace/sandbox
Update workspace sandbox overrides (can only add restrictions).

### GET /api/workspace/pi-resources
Get Pi resources for the workspace.

### POST /api/workspace/pi-resources
Apply Pi resources to the workspace.

---

## WebSocket

### GET /api/ws/mux
**Multiplexed WebSocket** -- the primary real-time endpoint. All channels are multiplexed over a single connection:

| Channel | Purpose |
|---------|---------|
| `agent` | Pi agent events and commands (prompt, abort, set_model, compact, fork) |
| `files` | File operations |
| `terminal` | Web terminal (ttyd) |
| `hstry` | Chat history events |
| `trx` | Issue tracking channel |

### GET /api/ws/debug
Debug info for WebSocket connections (public, no auth).

---

## Voice

### GET /api/voice/stt
WebSocket proxy to STT service (eaRS).

### GET /api/voice/tts
WebSocket proxy to TTS service (kokorox).

---

## Memories (mmry)

### Session-scoped memories

| Route | Method | Description |
|-------|--------|-------------|
| `/api/session/{id}/memories` | GET | List memories for session |
| `/api/session/{id}/memories` | POST | Add a memory |
| `/api/session/{id}/memories/search` | POST | Search memories |
| `/api/session/{id}/memories/stores` | GET | List memory stores |
| `/api/session/{id}/memories/{memory_id}` | GET/PUT/DELETE | CRUD on specific memory |

### Workspace-scoped memories

| Route | Method | Description |
|-------|--------|-------------|
| `/api/workspace/memories` | GET | List memories for workspace |
| `/api/workspace/memories` | POST | Add a memory |
| `/api/workspace/memories/search` | POST | Search memories |
| `/api/workspace/memories/{memory_id}` | GET/PUT/DELETE | CRUD on specific memory |

---

## Settings

Schema-driven configuration with hot-reload support.

### GET /api/settings/schema
Get the settings JSON schema (filtered by user role scope).

### GET /api/settings
Get current settings values.

### PATCH /api/settings
Update settings values.

### POST /api/settings/reload
Trigger hot-reload of settings.

---

## Agent Browser

### POST /api/browser/start
Start or navigate the agent-browser for a chat session. Uses the Pi/chat session ID to derive a browser session name.

### POST /api/browser/action
Send an action to the agent-browser.

### GET /api/sessions/{session_id}/browser/stream
WebSocket for browser screen stream (live screenshots).

---

## A2UI (Agent-to-UI)

Agent-to-UI protocol for agents to display interactive surfaces in the chat.

### POST /api/a2ui/surface
Send a UI surface to the user.

### DELETE /api/a2ui/surface/{session_id}/{surface_id}
Remove a displayed surface.

---

## Delegation

Localhost-only API (no auth) for Pi extensions to orchestrate sub-sessions.

### POST /api/delegate/start
Start a new delegate session.

### POST /api/delegate/prompt/{session_id}
Send a prompt to a delegate session.

### GET /api/delegate/status/{session_id}
Get status of a delegate session.

### GET /api/delegate/messages/{session_id}
Get messages from a delegate session.

### POST /api/delegate/stop/{session_id}
Stop a delegate session.

### GET /api/delegate/sessions
List all delegate sessions.

---

## Onboarding

Progressive onboarding system.

### GET /api/onboarding
Get current onboarding state.

### PUT /api/onboarding
Update onboarding state.

### GET /api/onboarding/check
Check if user needs onboarding.

### POST /api/onboarding/advance
Advance to next onboarding stage.

### POST /api/onboarding/unlock/{component}
Unlock a specific UI component.

### POST /api/onboarding/godmode
Skip all onboarding (unlock everything).

### POST /api/onboarding/complete
Mark onboarding as complete.

### POST /api/onboarding/reset
Reset onboarding state.

### POST /api/onboarding/bootstrap
Bootstrap onboarding from templates.

---

## Scheduler (sldr)

### GET /api/scheduler/overview
Get scheduler overview (all jobs).

### DELETE /api/scheduler/jobs/{name}
Delete a scheduled job.

### Sldr proxy

All `/api/sldr/*` routes are proxied to the sldr service (GET, POST, PUT, DELETE, PATCH).

---

## UI Control

Agent-driven UI control (used by agents and oqtoctl to control the frontend).

### POST /api/ui/navigate
Navigate to a route/path.

### POST /api/ui/session
Switch active session.

### POST /api/ui/view
Switch active view.

### POST /api/ui/palette
Open or close the command palette.

### POST /api/ui/palette/exec
Execute a palette command (new_chat, toggle_theme, etc.).

### POST /api/ui/spotlight
Spotlight a UI element.

### POST /api/ui/tour
Start a spotlight tour.

### POST /api/ui/sidebar
Collapse or expand the sidebar.

### POST /api/ui/panel
Control the right panel.

### POST /api/ui/theme
Switch the theme (light, dark, system).

---

## User Management

### GET /api/me
Get current user profile.

### PUT /api/me
Update current user profile.

---

## Admin Routes

All require admin role.

### Sessions
| Route | Method | Description |
|-------|--------|-------------|
| `/api/admin/sessions` | GET | List all sessions across all users |
| `/api/admin/sessions/{session_id}` | DELETE | Force stop/delete any session |
| `/api/admin/local/cleanup` | POST | Clean up orphan local sessions |

### Users
| Route | Method | Description |
|-------|--------|-------------|
| `/api/admin/users` | GET | List all users |
| `/api/admin/users` | POST | Create a new user |
| `/api/admin/users/sync-configs` | POST | Sync per-user configs (Pi, eavs) |
| `/api/admin/users/stats` | GET | Aggregate user statistics |
| `/api/admin/users/{user_id}` | GET/PUT/DELETE | CRUD on user |
| `/api/admin/users/{user_id}/activate` | POST | Activate user |
| `/api/admin/users/{user_id}/deactivate` | POST | Deactivate user |

### Invite Codes
| Route | Method | Description |
|-------|--------|-------------|
| `/api/admin/invite-codes` | GET | List all invite codes |
| `/api/admin/invite-codes` | POST | Create an invite code |
| `/api/admin/invite-codes/batch` | POST | Create multiple invite codes |
| `/api/admin/invite-codes/stats` | GET | Invite code statistics |
| `/api/admin/invite-codes/{code_id}` | GET/DELETE | Get or delete code |
| `/api/admin/invite-codes/{code_id}/revoke` | POST | Revoke a code |

### Eavs / Model Management
| Route | Method | Description |
|-------|--------|-------------|
| `/api/admin/eavs/providers` | GET | List eavs providers |
| `/api/admin/eavs/providers` | POST | Add/update eavs provider |
| `/api/admin/eavs/providers/{name}` | DELETE | Remove eavs provider |
| `/api/admin/eavs/sync-models` | POST | Sync models.json for all users |
| `/api/admin/eavs/catalog-lookup` | GET | Lookup model catalog |

### Metrics
| Route | Method | Description |
|-------|--------|-------------|
| `/api/admin/stats` | GET | Server statistics |
| `/api/admin/metrics` | GET | SSE stream of server metrics |

---

## Miscellaneous

### GET /api/health
Health check (public, no auth).

### GET /api/features
Feature flags and capabilities (public, no auth). Returns which features are enabled (voice, websocket_events, agent_browser, etc.).

### POST /api/feedback
Submit feedback/issues.

### GET /api/codexbar/usage
CodexBar usage statistics (requires `codexbar` binary on PATH).

### GET /api/feeds/fetch
Proxy for fetching RSS/Atom feeds.

---

## Test Harness (dev mode)

### POST /api/test/event
Send a mock event (for testing frontend rendering).

### POST /api/test/a2ui
Send a mock A2UI surface.

### POST /api/test/a2ui/sample
Send a sample A2UI surface.
