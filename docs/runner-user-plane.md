# Runner As Core User-Plane

This document describes the architecture for making octo-runner the mandatory user-plane boundary in local Linux multi-user mode.

## Overview

The goal is to ensure that in local Linux multi-user mode, the octo backend cannot directly access any user data. All user-plane operations (sessions, filesystem, memories, main chat state) are routed through per-user runner daemons, providing OS-level isolation.

### Current State

```
Backend (octo)
    |
    +-- SQLite DB (sessions, users, all data)
    +-- Direct filesystem access (workspaces)
    +-- Direct process spawn (opencode, pi, ttyd, fileserver)
    +-- Direct mmry access
```

### Target State

```
Backend (octo) [control-plane only]
    |
    +-- Control-plane DB (users, auth, routing only)
    +-- Per-user runner connections (Unix sockets)
    
Runner (per-user, runs as Linux user)
    |
    +-- User-plane DB (~/.local/share/octo/db.sqlite)
    +-- User filesystem access (workspace)
    +-- Process management (opencode, pi, ttyd, fileserver)
    +-- User mmry instance
    +-- Main chat session files
```

## Security Model

### Isolation Boundary

- Backend runs as a service user (e.g., `octo`)
- Each platform user has a corresponding Linux user (e.g., `octo_alice`)
- Per-user runner daemon runs as that Linux user via systemd user service
- Backend connects to runner via Unix socket at `/run/user/{uid}/octo-runner.sock`
- Socket permissions ensure only backend and the user can access it

### What Backend CAN Access

- Control-plane database (users, invites, auth tokens)
- Runner socket directory (to connect to per-user runners)
- Its own configuration files

### What Backend CANNOT Access

- User workspace directories (owned by per-user Linux accounts)
- Per-user databases (in user's home directory)
- User process memory or state
- Other users' runner sockets (enforced by Unix permissions)

## RPC Protocol Extensions

### New User-Plane Commands

The runner protocol is extended with these new request types:

```rust
pub enum RunnerRequest {
    // Existing process management
    SpawnProcess(SpawnProcessRequest),
    SpawnRpcProcess(SpawnRpcProcessRequest),
    KillProcess(KillProcessRequest),
    GetStatus(GetStatusRequest),
    ListProcesses,
    WriteStdin(WriteStdinRequest),
    ReadStdout(ReadStdoutRequest),
    SubscribeStdout(SubscribeStdoutRequest),
    Ping,
    Shutdown,
    
    // NEW: Session management
    ListSessions,
    GetSession(GetSessionRequest),
    CreateSession(CreateSessionRequest),
    StopSession(StopSessionRequest),
    DeleteSession(DeleteSessionRequest),
    
    // NEW: Filesystem operations
    ReadFile(ReadFileRequest),
    WriteFile(WriteFileRequest),
    ListDirectory(ListDirectoryRequest),
    CreateDirectory(CreateDirectoryRequest),
    DeletePath(DeletePathRequest),
    GetFileInfo(GetFileInfoRequest),
    
    // NEW: Memory/mmry operations
    ListMemories(ListMemoriesRequest),
    SearchMemories(SearchMemoriesRequest),
    AddMemory(AddMemoryRequest),
    DeleteMemory(DeleteMemoryRequest),
    
    // NEW: Main chat operations
    ListMainChatSessions,
    GetMainChatSession(GetMainChatSessionRequest),
    GetMainChatMessages(GetMainChatMessagesRequest),
    
    // NEW: Database operations (user-plane DB)
    DbQuery(DbQueryRequest),
    DbExecute(DbExecuteRequest),
}
```

### Per-User State Paths

The runner manages state at these locations within the user's home:

```
~/.local/share/octo/
    db.sqlite              # User's sessions, settings
    sessions/              # Claude Code session files
    main-chat/             # Pi session files
    mmry/                  # Memory database

~/.config/octo/
    config.toml            # User-specific config overrides
    sandbox.toml           # User sandbox restrictions (additive only)
```

## Implementation Plan

### Phase 1: Protocol Extension

1. Add new request/response types to `runner/protocol.rs`
2. Implement handlers in `bin/octo-runner.rs`
3. Add client methods to `runner/client.rs`

### Phase 2: User-Plane Trait

Create a `UserPlane` trait that abstracts user-plane operations:

```rust
#[async_trait]
pub trait UserPlane: Send + Sync {
    // Sessions
    async fn list_sessions(&self) -> Result<Vec<Session>>;
    async fn get_session(&self, id: &str) -> Result<Option<Session>>;
    async fn create_session(&self, req: CreateSessionRequest) -> Result<Session>;
    async fn stop_session(&self, id: &str) -> Result<()>;
    async fn delete_session(&self, id: &str) -> Result<()>;
    
    // Filesystem
    async fn read_file(&self, path: &Path, offset: Option<u64>, limit: Option<u64>) -> Result<Vec<u8>>;
    async fn write_file(&self, path: &Path, content: &[u8]) -> Result<()>;
    async fn list_directory(&self, path: &Path) -> Result<Vec<DirEntry>>;
    // ...
    
    // Memories
    async fn search_memories(&self, query: &str, limit: usize) -> Result<Vec<Memory>>;
    async fn add_memory(&self, content: &str, category: &str) -> Result<String>;
    // ...
    
    // Main Chat
    async fn list_main_chat_sessions(&self) -> Result<Vec<MainChatSession>>;
    // ...
}
```

Implementations:
- `DirectUserPlane` - Direct access (single-user mode, container mode)
- `RunnerUserPlane` - Via runner RPC (local multi-user mode)

### Phase 3: Backend Refactoring

1. Inject `UserPlane` into services that need user data access
2. Route operations through the trait instead of direct access
3. SessionService, MainChatPiService, file endpoints all use `UserPlane`

### Phase 4: Control-Plane DB Migration

1. Move session data to per-user DBs
2. Keep only control-plane data in backend DB:
   - Users table
   - Auth tokens
   - Invites
   - Port allocation (still needed for routing)
   
### Phase 5: User Provisioning

Update `octoctl user create` to:
1. Create Linux user with appropriate UID
2. Create home directory with skel
3. Initialize per-user state directories
4. Enable lingering for systemd user services
5. Start octo-runner as user service

## Configuration

### Backend Config

```toml
[backend]
mode = "local"           # or "container"

[backend.runner]
enabled = true           # Enable runner-as-user-plane
socket_dir = "/run/user" # Where to find per-user sockets
```

### Feature Flags

For gradual migration:

```toml
[features]
runner_user_plane = false  # Phase 1: disabled by default
```

## Streaming Support

The runner already supports stdout streaming via `SubscribeStdout`. This pattern extends to:

- Main chat streaming (Pi events)
- Session event streaming (OpenCode SSE)

The runner acts as a proxy, maintaining the streaming connection to the underlying process and forwarding events to the backend.

## Backwards Compatibility

- Single-user local mode: Uses `DirectUserPlane`, no runner required
- Container mode: Uses `DirectUserPlane` inside container, runner not used
- Multi-user local mode: Uses `RunnerUserPlane`, runner required

## Testing

### Security Tests

1. Create two users (alice, bob)
2. Verify alice's backend connection cannot read bob's files
3. Verify alice's runner cannot spawn processes as bob
4. Verify socket permissions prevent cross-user access

### Integration Tests

1. Full session lifecycle through runner
2. File operations through runner
3. Memory operations through runner
4. Streaming operations through runner

## Open Questions

1. **Port allocation**: Currently in backend DB. Should runner allocate ports?
   - Option A: Keep in backend (simpler, routing needs it anyway)
   - Option B: Runner allocates, reports back to backend
   
2. **EAVS integration**: Virtual keys are per-session. Runner needs access?
   - Runner doesn't need to know about EAVS - just passes env vars to processes
   
3. **mmry lifecycle**: Per-user mmry is already managed separately. Keep as-is?
   - Yes, but route through runner for access
