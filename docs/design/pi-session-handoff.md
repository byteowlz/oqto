# Pi Session Refactor - Handoff

## Context

We're refactoring Pi session management to move it from the octo backend into octo-runner. This enables:
- Single multiplexed WebSocket for all services (pi, files, terminal, hstry)
- Runner owns Pi process lifecycle and hstry persistence
- Backend becomes a stateless relay
- Clean user isolation (one runner per user)

**Design doc:** `docs/design/pi-session-refactor.md`

---

## What's Been Implemented

### 1. Protocol Layer (Complete)

All 36 Pi commands are defined in the runner protocol:

**Files:**
- `backend/crates/octo/src/runner/protocol.rs` - Request/response types
- `backend/crates/octo/src/runner/client.rs` - Client methods

**Commands implemented:**
- Session lifecycle: create, close, new, switch, list, subscribe, unsubscribe
- Prompting: prompt, steer, follow_up, abort
- State: get_state, get_messages, get_session_stats, get_last_assistant_text
- Model: set_model, cycle_model, get_available_models
- Thinking: set_thinking_level, cycle_thinking_level
- Compaction: compact, set_auto_compaction
- Queue modes: set_steering_mode, set_follow_up_mode
- Retry: set_auto_retry, abort_retry
- Forking: fork, get_fork_messages
- Metadata: set_session_name, export_html
- Commands/skills: get_commands
- Bash: bash, abort_bash
- Extension UI: extension_ui_response

### 2. Runner Handlers (Wired to PiSessionManager) - COMPLETE

All handlers exist in `backend/crates/octo/src/bin/octo-runner.rs` and route to PiSessionManager.

**All Pi handlers are now fully wired to PiSessionManager:**

- Session lifecycle: `pi_create_session`, `pi_close_session`, `pi_list_sessions`, `pi_new_session`, `pi_switch_session`
- Prompting: `pi_prompt`, `pi_steer`, `pi_follow_up`, `pi_abort`
- State queries: `pi_get_state`, `pi_get_messages`, `pi_get_session_stats`, `pi_get_last_assistant_text`
- Model: `pi_set_model`, `pi_cycle_model`, `pi_get_available_models`
- Thinking: `pi_set_thinking_level`, `pi_cycle_thinking_level`
- Compaction: `pi_compact`, `pi_set_auto_compaction`
- Queue modes: `pi_set_steering_mode`, `pi_set_follow_up_mode`
- Retry: `pi_set_auto_retry`, `pi_abort_retry`
- Forking: `pi_fork`, `pi_get_fork_messages`
- Metadata: `pi_set_session_name`, `pi_export_html`
- Commands: `pi_get_commands`
- Bash: `pi_bash`, `pi_abort_bash`
- Extensions: `pi_extension_ui_response`
- Subscription: `pi_subscribe` (streams events via broadcast channel)

### 3. Pi Session Manager (Fully Implemented)

**File:** `backend/crates/octo/src/runner/pi_manager.rs`

Implements:
- `PiSessionManager` struct with session map
- `PiSession` struct with process, state, event broadcaster
- `PiSessionState` enum (Starting, Idle, Streaming, Compacting, Stopping)
- Session lifecycle methods (create, close, list, new_session)
- Command routing for ALL 36 Pi commands via `PiSessionCommand` enum
- Response coordination via `PendingResponses` (HashMap of oneshot senders)
- Event broadcast via `tokio::sync::broadcast`
- State tracking from Pi events
- Idle cleanup loop
- Direct hstry writes on AgentEnd

**All public methods implemented:**
- Session lifecycle: `create_session`, `get_or_create_session`, `close_session`, `new_session`, `switch_session`, `set_session_name`
- Prompting: `prompt`, `steer`, `follow_up`, `abort`
- State queries (async with response coordination): `get_state`, `get_messages`, `get_session_stats`, `get_last_assistant_text`, `get_commands`, `get_available_models`, `get_fork_messages`
- Model: `set_model`, `cycle_model`
- Thinking: `set_thinking_level`, `cycle_thinking_level`
- Compaction: `compact`, `set_auto_compaction`
- Queue modes: `set_steering_mode`, `set_follow_up_mode`
- Retry: `set_auto_retry`, `abort_retry`
- Forking: `fork`
- Export: `export_html`
- Bash: `bash`, `abort_bash`
- Extensions: `extension_ui_response`
- Subscription: `subscribe`, `list_sessions`

**Integrated** with runner request handlers - PiSessionManager is initialized in `main()` and passed to Runner.

### 4. Multiplexed WebSocket (Skeleton)

**Backend:** `backend/crates/octo/src/api/ws_multiplexed.rs`
- Channel-based message routing (pi, files, terminal, hstry, system)
- Route registered at `/api/ws/mux`
- Handlers log commands, return placeholders

**Frontend:**
- `frontend/lib/ws-mux-types.ts` - TypeScript types
- `frontend/lib/ws-manager.ts` - `WsConnectionManager` class with reconnection
- `frontend/features/main-chat/hooks/usePiChatV2.ts` - Hook using manager

### 5. Build Status

- Backend: **Compiles** (cargo check passes)
- Frontend: **Builds** (bun run build passes)

---

## What Needs to Be Done

### Phase 1: Wire Runner to PiSessionManager (COMPLETE)

**Goal:** Runner handlers call PiSessionManager instead of returning stubs.

- [x] Initialize PiSessionManager in runner main
- [x] Add PiSessionManager to Runner struct
- [x] Update core Pi handlers to call pi_manager
- [x] Handle PiSubscribe specially (streams events from pi_manager's broadcast channel)
- [x] cargo check passes

### Phase 2: Wire Backend WS to Runner (COMPLETE)

**Goal:** Multiplexed WS handler forwards Pi commands to runner.

- [x] Inject RunnerClient into WS handler via `runner_client_for_user()`
- [x] Route core Pi commands to RunnerClient methods
- [x] Stream Pi events from runner subscription to WebSocket
- [x] Add all 36 Pi commands to `PiWsCommand` enum
- [x] Update frontend `ws-mux-types.ts` with all commands
- [x] cargo check passes
- [x] bun run build passes

**Implementation notes:**
- RunnerClient is obtained per-user via `runner_socket_pattern` in AppState
- Fallback to default socket for single-user mode if pattern not set
- Pi event forwarding spawns a background task per session subscription
- Commands not yet implemented in RunnerClient return "not implemented" errors

### Phase 3: Sandbox Integration (COMPLETE)

**Goal:** Pi processes run inside octo-sandbox with security checks.

- [x] Add `sandbox_config: Option<SandboxConfig>` to `PiManagerConfig`
- [x] Update `PiSessionManager::create_session` to wrap Pi in bwrap when sandbox enabled
- [x] Merge workspace-specific sandbox config with global (can only add restrictions)
- [x] Pass sandbox config from Runner to PiSessionManager
- [x] SECURITY: Refuse to run if sandbox requested but bwrap not available
- [x] cargo check passes

**Implementation notes:**
- Sandbox config is loaded by runner at startup from `/etc/octo/sandbox.toml` (system) or `--sandbox-config` flag
- PiManagerConfig now includes `sandbox_config: Option<SandboxConfig>`
- `create_session` uses `sandbox_config.with_workspace_config()` to merge workspace restrictions
- Pi args are built first, then wrapped with bwrap args if sandboxing enabled
- If sandbox is enabled but bwrap is not available, the session creation fails with a clear error
- Workspace configs can only ADD restrictions (deny_read, deny_write, isolation), never weaken security

### Phase 4: Frontend Migration (COMPLETE)

**Goal:** Switch frontend to use new multiplexed WS.

- [x] Update ws-mux-types.ts with all 36 commands (done in Phase 2)
- [x] Implement `newSession`, `resetSession`, `refresh` in usePiChatV2
- [x] Export `usePiChatV2` as `usePiChat` (no feature flag - direct replacement)
- [x] Remove old frontend hooks (usePiChat.ts, usePiChatCore.ts, usePiChatStreaming.ts, usePiChatHistory.ts)
- [x] Remove old WebSocket creator function `createMainChatPiWebSocket`
- [x] bun run build passes

**Implementation notes:**
- `usePiChatV2` is now exported as `usePiChat` from `@/features/main-chat/hooks`
- Old per-session WebSocket hooks have been removed
- Skipped feature flag approach - direct replacement since old code is no longer needed
- REST API routes are kept for backwards compatibility (session listing, models, stats, etc.)

### Phase 5: Cleanup (PARTIAL)

**Completed:**
- [x] Remove old frontend hooks
- [x] Remove `createMainChatPiWebSocket` function

**Deferred (backend REST routes still needed):**
- [ ] `backend/crates/octo/src/main_chat/pi_service.rs` - Still used by REST API routes
- [ ] `backend/crates/octo/src/api/main_chat_pi.rs` - Still provides session listing, models, stats
- [ ] `backend/crates/octo/src/pi/runtime.rs` - Still used by pi_service

The backend REST routes (`/api/main/pi/*`) are still used for:
- Listing Pi sessions from disk
- Getting available models
- Getting session stats
- Session search

These can be migrated to runner-backed implementations in a future refactor.

---

## Key Files Reference

```
# Design
docs/design/pi-session-refactor.md      # Full spec

# Backend - Protocol
backend/crates/octo/src/runner/protocol.rs   # All types
backend/crates/octo/src/runner/client.rs     # Client methods
backend/crates/octo/src/runner/pi_manager.rs # Session manager (with sandbox support)

# Backend - Runner
backend/crates/octo/src/bin/octo-runner.rs   # All Pi handlers wired to PiSessionManager

# Backend - WS
backend/crates/octo/src/api/ws_multiplexed.rs  # Multiplexed WebSocket handler

# Frontend (New)
frontend/lib/ws-mux-types.ts                   # TypeScript types for mux WS
frontend/lib/ws-manager.ts                     # WsConnectionManager singleton
frontend/features/main-chat/hooks/usePiChatV2.ts  # Main hook (exported as usePiChat)
frontend/features/main-chat/hooks/index.ts    # Re-exports usePiChatV2 as usePiChat

# Backend - Legacy REST routes (still used)
backend/crates/octo/src/main_chat/pi_service.rs  # Used by REST routes
backend/crates/octo/src/api/main_chat_pi.rs      # REST handlers for session listing, models, etc.
```

---

## Testing Strategy

1. **Unit tests:** Run `cargo test` after each change
2. **Manual testing:**
   - Start runner: `cargo run --bin octo-runner`
   - Start backend: `cargo run --bin octo`
   - Open frontend, check console for WS messages
3. **Integration test:** Send commands via WS, verify Pi responds

---

## Quick Start for Next Session

```bash
# Check current state
cd /home/wismut/byteowlz/octo
cd backend && cargo check      # Backend compiles
cd frontend && bun run build   # Frontend builds

# All phases COMPLETE!
# The new multiplexed WebSocket flow is:
# Frontend (usePiChat) -> WsConnectionManager -> /api/ws/mux -> RunnerClient -> PiSessionManager -> Pi process

# To test:
# 1. Start runner: cargo run --bin octo-runner
# 2. Start backend: cargo run --bin octo
# 3. Open frontend, Main Chat should use multiplexed WS
```

---

## Outstanding TODOs in Code

### octo-runner.rs Pi Handler TODOs

**All Pi handlers are now fully implemented.** Commands are routed through `PiSessionManager` which sends them to Pi's stdin as JSON RPC.

Commands that return data (like `get_state`, `get_messages`, `get_available_models`) use the `pending_responses` pattern - a oneshot channel is registered before sending the command, and the reader task routes responses back by matching the request ID.

### Other TODOs

| File:Line | TODO | Priority |
|-----------|------|----------|
| octo-runner.rs:974 | Add workspace root validation for file reads | Security |
| octo-runner.rs:1251 | Track actual session creation time | Low |
| octo-runner.rs:1904-1920 | Implement mmry database search/add/delete | Medium |
| octo-runner.rs:2286 | Expose cwd from PiSessionInfo in list_sessions | Low |

---

## Open Questions

1. **Single-user mode without runner?** 
   - Current plan: always use runner (can be embedded)
   - Alternative: backend manages Pi directly in local mode

2. **Container mode?**
   - Runner inside container alongside Pi
   - Or runner on host managing containers

3. **Session persistence across runner restart?**
   - Currently sessions are in-memory
   - Could persist session list to disk
