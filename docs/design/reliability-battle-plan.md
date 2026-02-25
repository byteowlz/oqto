# Reliability & User Management Battle Plan

**Date**: 2025-02-25
**Status**: Draft
**Priority**: P0 (blocking multi-machine deployment)

---

## Problem Statement

Deploying Oqto across multiple machines has surfaced three categories of issues:

1. **User Management Gaps** -- `oqtoctl` lacks password reset, role changes, and other basic user lifecycle operations.
2. **Message Loss / Error Propagation** -- Error messages from Pi/LLM providers are not reliably reaching the frontend. The full streaming pipeline (LLM -> eavs -> Pi RPC -> runner translator -> WebSocket -> frontend) has multiple failure modes that silently swallow errors.
3. **Agent Crash Recovery** -- Pi crashes (potentially caused by the octo-todos TUI extension) leave sessions in a broken state with no recovery path.

---

## Workstream 1: Enhanced Eavs Mock Provider (PRIORITY: HIGHEST)

### Why This Is First

We cannot reliably test or debug the streaming pipeline without a deterministic, controllable mock provider. The existing `mock` provider in eavs returns a single static "This is a mock response for benchmarking" message. We need something much more capable.

### Requirements

The enhanced mock provider must:

1. **Simulate realistic streaming** -- Token-by-token SSE chunks with configurable delays (matching real provider latency patterns)
2. **Support tool calls** -- Return properly formatted tool_call chunks (function name + arguments streamed), followed by stop_reason=tool_calls, so Pi enters its agentic loop
3. **Support multi-turn conversations** -- Track conversation context to produce appropriate responses (e.g., after a tool result, generate the next assistant turn)
4. **Simulate error conditions** -- Return 429 (rate limit), 500 (server error), 503 (overloaded), partial streams that cut off mid-token, malformed JSON chunks, connection resets
5. **Be configurable via request headers or eavs config** -- Scenarios like: `X-Mock-Scenario: rate_limit_after_3_chunks`, `X-Mock-Scenario: tool_call_then_text`, `X-Mock-Delay-Ms: 50`
6. **Log every chunk sent** -- Full audit trail for debugging what eavs sent vs what the frontend received

### Implementation Plan

Location: `eavs/src/mock_provider.rs` (extract from inline in proxy.rs)

Scenarios to implement:
- `simple_text` -- Stream "Hello, I can help with that." word by word
- `long_text` -- Stream 500+ tokens to test backpressure
- `tool_call` -- Emit a tool_call for `Read` with file path argument
- `multi_tool` -- Emit two sequential tool_calls
- `error_mid_stream` -- Stream 3 chunks then return an error chunk
- `rate_limit` -- Return 429 with Retry-After header
- `server_error` -- Return 500
- `timeout` -- Accept request, never respond (test client timeout handling)
- `malformed_sse` -- Return broken SSE formatting
- `connection_reset` -- Drop TCP connection mid-stream
- `thinking` -- Stream thinking/reasoning blocks (for models that support it)

### Testing Integration

Once the mock provider exists:
1. Configure eavs with a `mock` provider entry pointing to the internal handler
2. Add mock models to models.json (e.g., `mock/simple-text`, `mock/tool-call`, `mock/error-mid-stream`)
3. Write integration tests that:
   - Send a prompt via WebSocket
   - Verify every streamed chunk arrives at the frontend
   - Verify error events are properly displayed
   - Verify agent state transitions (idle -> working -> idle or idle -> working -> error)

---

## Workstream 2: End-to-End Streaming Reliability Tests

### The Full Pipeline

```
User prompt (frontend)
  -> WebSocket {channel: "agent", event: "prompt"}
  -> Backend ws_multiplexed.rs
  -> Runner socket (BackendToRunner::Command)
  -> pi_manager.rs: write to Pi stdin
  -> Pi process: send to LLM via eavs
  -> eavs: mock provider or real provider
  -> Pi stdout: JSONL events
  -> pi_manager.rs: stdout_reader_task (StreamDeserializer)
  -> pi_translator.rs: PiEvent -> Vec<EventPayload>
  -> broadcast::Sender<CanonicalEvent>
  -> Backend: RunnerUserPlane reads from runner socket
  -> Backend: WebSocket handler sends to client
  -> Frontend: useChat.ts processes events
  -> Frontend: DisplayMessage rendered in UI
```

### Known Failure Points (from memories)

| Point | Issue | Status |
|-------|-------|--------|
| Pi stdout buffering | Multiple JSON objects concatenated on one line (4096 byte buffer) | FIXED (StreamDeserializer) |
| hstry persistence timing | persist happened AFTER agent.idle broadcast | FIXED (persist BEFORE broadcast) |
| hstry compaction corruption | Compacted messages overwrote originals by idx | FIXED (guard on message count) |
| Message merge mode | Frontend used count heuristics instead of explicit mode | FIXED (explicit MergeMode) |

### New Failure Points to Investigate

| Point | Hypothesis | Test Strategy |
|-------|-----------|---------------|
| Error events not reaching frontend | `AgentError` from translator may not be forwarded through runner socket | Mock error scenario + WebSocket capture |
| Pi extension crash | octo-todos TUI widget crash causes Pi process exit | Test with intentional widget error |
| Runner-to-backend event loss | broadcast channel overflow (capacity 1024) during rapid streaming | Flood test with mock provider |
| WebSocket backpressure | Slow frontend causes WebSocket buffer to fill, events dropped | Throttled client test |
| Reconnect after crash | Frontend does not re-fetch state after agent.error with recoverable=false | Crash + reconnect scenario |
| Session stuck in "working" | Pi crashes without emitting agent_end, state never returns to idle | Process kill test |

### Test Harness Design

Create `scripts/e2e-streaming-test.sh` and/or a Rust integration test that:

1. Starts eavs with mock provider
2. Starts a Pi instance in RPC mode (or through the runner)
3. Connects a WebSocket client to oqto backend
4. Sends prompts and captures ALL events
5. Compares captured events against expected event sequence
6. Reports any missing events, state violations, or timing anomalies

For frontend tests, extend the existing `test_harness.rs` to inject mock events and verify the frontend's reaction via `agent-browser`.

---

## Workstream 3: Octo-Todos Extension Crash Investigation

### Symptoms

Pi crashes with an error message mentioning "TUI". The octo-todos extension at `~/.pi/agent/extensions/octo-todos/index.ts` uses:
- `ctx.ui.setWidget()` -- TUI widget rendering
- `ctx.hasUI` checks
- Session event handlers (session_start, session_switch, session_fork, session_tree)

### Hypotheses

1. **Widget render after session destroy** -- If a session is destroyed while the widget is being updated, the TUI render call may access freed state.
2. **Widget render during RPC mode** -- Pi in `--mode rpc` may have `hasUI=true` but the TUI backend is not actually initialized, causing null pointer / missing renderer.
3. **Exception in event handler** -- If `reconstructTodos` throws (e.g., file system error reading todos JSON), the unhandled exception may crash Pi.
4. **Race between session events** -- Multiple rapid session_switch events cause concurrent widget updates that conflict.

### Investigation Steps

1. **Reproduce the crash** -- Run Pi with the extension in RPC mode, send prompts, check for TUI-related stack traces in stderr
2. **Add error boundaries** -- Wrap all event handlers and widget callbacks in try/catch
3. **Guard hasUI more carefully** -- Check if `ctx.hasUI` is reliable in RPC mode or if the extension should detect RPC mode and skip TUI entirely
4. **Test without extension** -- Temporarily disable octo-todos and verify Pi stability to confirm it's the cause

### Immediate Fix

Add defensive error handling to the extension:

```typescript
// Wrap all event handlers
pi.on("session_start", async (_event, ctx) => {
  try { reconstructTodos(ctx); } catch (e) { /* log but don't crash */ }
});

// Wrap widget factory
function updateWidget(ctx: ExtensionContext): void {
  if (!ctx.hasUI) return;
  try {
    ctx.ui.setWidget(WIDGET_KEY, (_tui, theme) => {
      try {
        const lines = buildWidgetLines(_currentTodos, theme);
        return { render: () => lines, invalidate: () => {} };
      } catch {
        return { render: () => ["[todo widget error]"], invalidate: () => {} };
      }
    });
  } catch { /* swallow -- TUI not available */ }
}
```

---

## Workstream 4: User Management in oqtoctl

### Current State

`oqtoctl user` supports: `create`, `list`, `show`, `setup-runner`, `runner-status`, `delete`, `sync-configs`, `bootstrap`

### Missing Operations

| Command | Description | Implementation |
|---------|-------------|----------------|
| `oqtoctl user set-password <user>` | Reset/change password | Hash new password with bcrypt, UPDATE users SET password_hash |
| `oqtoctl user set-role <user> <role>` | Change user role (user/admin) | UPDATE users SET role, invalidate JWT sessions |
| `oqtoctl user disable <user>` | Disable without deleting | UPDATE users SET is_active=0, kill active sessions |
| `oqtoctl user enable <user>` | Re-enable a disabled user | UPDATE users SET is_active=1 |
| `oqtoctl user set-email <user> <email>` | Update email | UPDATE users SET email, check uniqueness |
| `oqtoctl user set-display-name <user> <name>` | Update display name | UPDATE users SET display_name |
| `oqtoctl user sessions <user>` | List active sessions for user | Query runner for active Pi sessions |
| `oqtoctl user kill-sessions <user>` | Kill all active agent sessions | Send abort to all sessions, force-kill Pi processes |
| `oqtoctl user reprovision <user>` | Re-run eavs + models.json + runner setup | Combines eavs key rotation + sync-configs |

### Implementation Priority

1. `set-password` -- Most urgent for multi-machine deployment
2. `disable` / `enable` -- Needed for security
3. `set-role` -- Needed for delegating admin access
4. `sessions` / `kill-sessions` -- Needed for debugging
5. `reprovision` -- Convenience for deployment automation

### API Endpoints Needed

Some of these can work directly against the DB (like bootstrap does), but for consistency we should add admin API endpoints:

- `PATCH /api/admin/users/:id` -- Update user fields (password_hash, role, is_active, email, display_name)
- `GET /api/admin/users/:id/sessions` -- List active sessions
- `POST /api/admin/users/:id/kill-sessions` -- Force-kill all sessions

---

## Workstream 5: Crash Recovery & Session Resilience

### Current Behavior When Pi Crashes

1. Runner's stdout_reader_task sees EOF
2. `on_process_exit()` emits `AgentError { recoverable: false }`
3. Session state transitions to `Dead`
4. Frontend shows error

### What's Missing

1. **No auto-respawn** -- The session stays dead. User must create a new session.
2. **No crash diagnostics** -- Pi's stderr output is not captured/forwarded to the frontend
3. **No "resume" capability** -- After crash, the conversation is in hstry but there's no way to start a new Pi process attached to that conversation
4. **Stale session cleanup** -- Dead sessions linger in the runner's session map

### Proposed Improvements

1. **Capture Pi stderr** -- Pipe stderr to a ring buffer, include last N lines in the `AgentError` event
2. **Session "reconnect" command** -- Frontend can send a `reconnect` command that spawns a new Pi process for the same session, loading history from hstry
3. **Auto-respawn with backoff** -- If Pi crashes within 5s of start, it's likely a config issue (don't respawn). Otherwise, auto-respawn once with a 2s delay.
4. **Dead session cleanup** -- Runner periodically checks for Dead sessions and cleans up after 5 minutes

---

## Execution Order

### Phase 1: Foundation (Week 1)
1. [x -> WS1] Enhance eavs mock provider with realistic scenarios
2. [x -> WS3] Add error boundaries to octo-todos extension
3. [x -> WS4] Implement `oqtoctl user set-password`

### Phase 2: Testing Infrastructure (Week 1-2)
4. [x -> WS2] Build e2e streaming test harness using mock provider
5. [x -> WS2] Test and fix error propagation for each failure point
6. [x -> WS4] Implement remaining user management commands

### Phase 3: Resilience (Week 2-3)
7. [x -> WS5] Capture Pi stderr in crash events
8. [x -> WS5] Implement session reconnect command
9. [x -> WS5] Auto-respawn with backoff

### Phase 4: Hardening (Ongoing)
10. Continuous integration tests using mock provider
11. Chaos testing (random process kills, network partitions)
12. Frontend reconnection stress tests

---

## Success Criteria

- [ ] Every error from LLM provider reaches the frontend as a visible error message
- [ ] Pi crash shows the actual error to the user and offers recovery
- [ ] Admin can reset any user's password via oqtoctl without server restart
- [ ] E2E test suite runs in CI and catches regressions in streaming pipeline
- [ ] Mock provider enables testing without any real LLM API keys
- [ ] Sessions auto-recover after transient Pi crashes
