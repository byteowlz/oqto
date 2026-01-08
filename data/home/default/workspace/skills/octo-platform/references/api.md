# Octo API Reference

## Table of Contents

1. [Authentication](#authentication)
2. [Sessions](#sessions)
3. [Proxy Routes](#proxy-routes)
4. [User Management](#user-management)
5. [Admin Routes](#admin-routes)
6. [Voice](#voice)
7. [Memories](#memories)

---

## Authentication

### POST /api/auth/login
Login with email and password.

**Request:**
```json
{
  "email": "user@example.com",
  "password": "password"
}
```

**Response:** JWT token set as cookie.

### POST /api/auth/register
Register with invite code.

**Request:**
```json
{
  "email": "user@example.com",
  "password": "password",
  "invite_code": "ABC123"
}
```

### POST /api/auth/logout
Clear authentication cookie.

### GET /api/auth/check
Check if current session is authenticated.

---

## Sessions

### GET /api/sessions
List all sessions for current user.

**Response:**
```json
[
  {
    "id": "session_id",
    "name": "project-name",
    "status": "running",
    "created_at": "2024-01-01T00:00:00Z",
    "project_path": "/path/to/project"
  }
]
```

### POST /api/sessions
Create a new session.

**Request:**
```json
{
  "project_path": "/path/to/project",
  "name": "optional-name"
}
```

**Response:** Session object with id, ports, status.

### GET /api/sessions/{session_id}
Get session details.

### DELETE /api/sessions/{session_id}
Delete a session (stops container if running).

### POST /api/sessions/{session_id}/stop
Stop a running session (preserves container state).

### POST /api/sessions/{session_id}/resume
Resume a stopped session.

### POST /api/sessions/{session_id}/upgrade
Upgrade session to latest container image.

### GET /api/sessions/updates
Check if updates are available for any sessions.

---

## Proxy Routes

All proxy routes forward requests to session services.

### OpenCode Proxy
| Route | Description |
|-------|-------------|
| `GET/POST/PUT/DELETE /api/session/{id}/code/*` | Proxy to OpenCode API |
| `GET /api/session/{id}/code/event` | SSE event stream |

### Fileserver Proxy
| Route | Description |
|-------|-------------|
| `GET/POST/PUT/DELETE /api/session/{id}/files/*` | Proxy to fileserver |
| `GET/POST/PUT/DELETE /api/session/{id}/workspace/*` | Workspace-specific file operations |

### Terminal Proxy
| Route | Description |
|-------|-------------|
| `GET /api/session/{id}/term` | WebSocket to ttyd terminal |

---

## User Management

### GET /api/me
Get current user profile.

### PUT /api/me
Update current user profile.

**Request:**
```json
{
  "display_name": "New Name"
}
```

---

## Admin Routes

Requires admin role.

### GET /api/admin/sessions
List all sessions across all users.

### DELETE /api/admin/sessions/{session_id}
Force delete any session.

### GET /api/admin/users
List all users.

### POST /api/admin/users
Create a new user.

### GET /api/admin/users/{user_id}
Get user details.

### PUT /api/admin/users/{user_id}
Update user.

### DELETE /api/admin/users/{user_id}
Delete user.

### POST /api/admin/users/{user_id}/promote
Promote user to admin.

### POST /api/admin/users/{user_id}/demote
Demote admin to user.

### GET /api/admin/invite-codes
List all invite codes.

### POST /api/admin/invite-codes
Create a new invite code.

**Request:**
```json
{
  "max_uses": 10,
  "expires_at": "2024-12-31T23:59:59Z"
}
```

### DELETE /api/admin/invite-codes/{code_id}
Delete an invite code.

### GET /api/admin/metrics
SSE stream of server metrics.

### GET /api/admin/users/stats
Get aggregate user statistics.

---

## Voice

### GET /api/voice/stt
WebSocket proxy to STT service (eaRS).

### GET /api/voice/tts
WebSocket proxy to TTS service (kokorox).

---

## Memories

Proxy to mmry service for session memory.

### GET /api/session/{id}/mmry
List memories for session.

### POST /api/session/{id}/mmry
Add a memory.

### GET /api/session/{id}/mmry/{memory_id}
Get specific memory.

### PUT /api/session/{id}/mmry/{memory_id}
Update memory.

### DELETE /api/session/{id}/mmry/{memory_id}
Delete memory.

### POST /api/session/{id}/mmry/search
Search memories.

**Request:**
```json
{
  "query": "search term",
  "limit": 10
}
```

---

## WebSocket

### GET /api/ws
Main WebSocket endpoint for real-time communication.

Used for:
- Session status updates
- Chat messages
- Tool call notifications
- Progress events
