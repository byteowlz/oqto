# Canonical Protocol Specification

Status: DRAFT
Date: 2026-02-05

## Overview

This document defines the canonical message, event, and command formats used across all Octo communication boundaries:

```
Frontend <--[WS: canonical msgs/events]--> Backend <--[canonical stream]--> Runner(s)
                                                                              |
                                                                        Agent Harness
                                                                        (pi, opencode, ...)
```

The frontend speaks only the canonical protocol. It does not know or care which agent harness is running. The runner is responsible for translating native agent protocols into the canonical format.

A user can have multiple runners: one on the hub server, others on VMs, remote machines, or their local workstation. The backend routes to the correct runner based on session/workspace configuration.

## Design Principles

1. **Messages are persistent, events are ephemeral.** Messages represent conversation content stored in hstry. Events represent transient status signals that drive the UI but are not persisted as conversation content.

2. **The canonical format is the source of truth.** hstry stores canonical messages directly. The frontend renders canonical messages directly. No translation at consumption time.

3. **Parts are the atomic content unit.** A message contains an ordered list of typed parts. This aligns with hstry's existing `Part` enum and eavs' `ContentBlock` enum.

4. **Events form a state machine.** The frontend can always derive the exact UI state from the current event without tracking history. Each event is self-describing.

5. **Agent-agnostic.** The protocol supports any agent harness. Agent-specific features are exposed through the extension mechanism (optional parts, optional event fields, `x-*` extension types).

6. **Runner-scoped sessions.** Each session belongs to exactly one runner. The backend routes commands to the correct runner. The frontend addresses sessions by `(runner_id, session_id)`.

---

## Part 1: Canonical Message Format

Messages are the persistent units of a conversation. They are stored in hstry and rendered by the frontend.

### Message Envelope

```typescript
type Message = {
  id: string;                    // unique within conversation (uuid or agent-assigned)
  idx: number;                   // 0-based position in conversation
  role: "user" | "assistant" | "system" | "tool";
  parts: Part[];                 // ordered content blocks
  created_at: number;            // unix ms

  // Assistant-specific (null for other roles)
  model?: string;                // e.g. "claude-sonnet-4-20250514"
  provider?: string;             // e.g. "anthropic"
  stop_reason?: StopReason;      // why generation stopped
  usage?: Usage;                 // token counts

  // Tool-result-specific (null for other roles)
  tool_call_id?: string;         // correlates to the ToolCall part
  tool_name?: string;
  is_error?: boolean;

  metadata?: Record<string, unknown>;  // agent-specific extras
};

type StopReason = "stop" | "length" | "tool_use" | "error" | "aborted";

type Usage = {
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens?: number;
  cache_write_tokens?: number;
  cost_usd?: number;
};
```

### Part Types

Parts are the atomic content blocks within a message. Tagged union on `type`.

```typescript
type Part =
  // --- Core (mandatory support) ---
  | { type: "text";        id: string; text: string; format?: "markdown" | "plain" }
  | { type: "thinking";    id: string; text: string }
  | { type: "tool_call";   id: string; tool_call_id: string; name: string;
      input?: unknown; status: ToolStatus }
  | { type: "tool_result"; id: string; tool_call_id: string; name?: string;
      output?: unknown; is_error: boolean; duration_ms?: number }

  // --- Media ---
  | { type: "image";       id: string; source: MediaSource; alt?: string }
  | { type: "audio";       id: string; source: MediaSource; duration_sec?: number;
      transcript?: string }
  | { type: "video";       id: string; source: MediaSource; duration_sec?: number }
  | { type: "file_ref";    id: string; uri: string; label?: string;
      range?: FileRange }
  | { type: "attachment";  id: string; source: MediaSource; filename?: string;
      size_bytes?: number }

  // --- Extensions ---
  | { type: `x-${string}`; id: string; payload?: unknown;
      meta?: Record<string, unknown> };

type ToolStatus = "pending" | "running" | "success" | "error";

type MediaSource =
  | { source: "url"; url: string; mime_type?: string }
  | { source: "attachment_ref"; attachment_id: string; mime_type?: string }
  | { source: "base64"; data: string; mime_type: string };

type FileRange = {
  start_line?: number;
  end_line?: number;
};
```

### Alignment with hstry

The canonical message maps directly to hstry's storage:

| Canonical Field | hstry Column | Notes |
|----------------|--------------|-------|
| `id` | `messages.id` | UUID |
| `idx` | `messages.idx` | conversation ordering |
| `role` | `messages.role` | same enum values |
| `parts` | `messages.parts_json` | serialized Part[] |
| `created_at` | `messages.created_at` | unix ms |
| `model` | `messages.model` | |
| `usage.cost_usd` | `messages.cost_usd` | |
| `usage.*_tokens` | `messages.tokens` | summed for legacy column |
| (flattened text) | `messages.content` | projected from text/thinking parts for FTS |

The hstry `Part` enum already matches this spec. The only changes needed:
- Add `x-*` extension variant to Rust `Part` enum (already in TS adapter types)
- Add `meta` field to all Part variants (already in TS adapter types)
- Ensure `ToolStatus` has `Running` variant (already in hstry `parts.rs`)

### Alignment with eavs

eavs' `ContentBlock` maps to canonical parts:

| eavs ContentBlock | Canonical Part |
|-------------------|----------------|
| `Text { text }` | `{ type: "text", text }` |
| `Thinking { thinking }` | `{ type: "thinking", text: thinking }` |
| `ToolCall { id, name, arguments }` | `{ type: "tool_call", tool_call_id: id, name, input: arguments }` |
| `ToolResult { .. }` | `{ type: "tool_result", .. }` |
| `Image { source }` | `{ type: "image", source }` |

eavs' `StreamEvent` maps to canonical streaming events (see Part 2).

---

## Part 2: Canonical Event Format

Events are ephemeral signals for real-time UI updates. They are NOT stored in hstry as messages (but some may be logged for debugging).

### Event Envelope

Every event has:

```typescript
type Event = {
  session_id: string;           // which session this event belongs to
  runner_id: string;            // which runner produced it
  ts: number;                   // unix ms timestamp
} & EventPayload;
```

### Session Lifecycle Events

```typescript
// Session created/resumed on the runner
| { event: "session.created"; session_id: string; resumed: boolean;
    harness: string; runner_id: string }

// Session stopped/destroyed
| { event: "session.closed"; session_id: string; reason?: string }

// Session health -- emitted periodically by the runner (every ~10s)
| { event: "session.heartbeat"; session_id: string;
    process: { alive: boolean; pid?: number; rss_bytes?: number;
               cpu_pct?: number; uptime_s?: number } }
```

### Agent State Events

These form a clear state machine. The frontend can derive its UI state from any single event.

```typescript
// Agent is idle, waiting for input
| { event: "agent.idle" }

// Agent is working (LLM generating, tool running, etc.)
// `phase` tells the frontend WHAT is happening
| { event: "agent.working"; phase: AgentPhase; detail?: string }

// Agent encountered an error
| { event: "agent.error"; error: string; recoverable: boolean;
    phase?: AgentPhase }

// Agent needs user input (extension dialog, permission, question)
| { event: "agent.input_needed"; request: InputRequest }

// Agent input request resolved
| { event: "agent.input_resolved"; request_id: string }

type AgentPhase =
  | "generating"       // LLM is producing tokens
  | "thinking"         // LLM is in extended thinking
  | "tool_running"     // tool is executing
  | "compacting"       // context compaction in progress
  | "retrying"         // auto-retry after transient error
  | "initializing";    // session starting up

type InputRequest =
  | { type: "select";  request_id: string; title: string; options: string[];
      timeout?: number }
  | { type: "confirm"; request_id: string; title: string; message: string;
      timeout?: number }
  | { type: "input";   request_id: string; title: string;
      placeholder?: string; timeout?: number }
  | { type: "permission"; request_id: string; title: string;
      description?: string; metadata?: unknown };
```

### Streaming Events

These deliver incremental content as the agent generates it.

```typescript
// New message started (role tells frontend what kind of bubble to create)
| { event: "stream.message_start"; message_id: string; role: string }

// Text content delta
| { event: "stream.text_delta"; message_id: string; delta: string;
    content_index: number }

// Thinking content delta
| { event: "stream.thinking_delta"; message_id: string; delta: string;
    content_index: number }

// Tool call being assembled by LLM
| { event: "stream.tool_call_start"; message_id: string;
    tool_call_id: string; name: string; content_index: number }
| { event: "stream.tool_call_delta"; message_id: string;
    tool_call_id: string; delta: string; content_index: number }
| { event: "stream.tool_call_end"; message_id: string;
    tool_call_id: string; tool_call: { id: string; name: string;
    input: unknown }; content_index: number }

// Message complete -- includes the full finalized message
| { event: "stream.message_end"; message: Message }

// Stream complete (agent may continue with tools, or go idle)
| { event: "stream.done"; reason: StopReason }
```

### Tool Execution Events

Separate from streaming (tool execution happens AFTER the LLM decides to call a tool).

```typescript
// Tool started
| { event: "tool.start"; tool_call_id: string; name: string;
    input?: unknown }

// Tool progress (accumulated output, not delta)
| { event: "tool.progress"; tool_call_id: string; name: string;
    partial_output: unknown }

// Tool completed
| { event: "tool.end"; tool_call_id: string; name: string;
    output: unknown; is_error: boolean; duration_ms?: number }
```

### Auto-Recovery Events

```typescript
// Auto-retry starting
| { event: "retry.start"; attempt: number; max_attempts: number;
    delay_ms: number; error: string }

// Auto-retry result
| { event: "retry.end"; success: boolean; attempt: number;
    final_error?: string }

// Auto-compaction starting
| { event: "compact.start"; reason: "threshold" | "overflow" }

// Auto-compaction result
| { event: "compact.end"; success: boolean; will_retry: boolean;
    error?: string }
```

### Model/Config Change Events

```typescript
| { event: "config.model_changed"; provider: string; model_id: string }
| { event: "config.thinking_level_changed"; level: string }
```

### Notification Events

For extension-originated notifications and status updates.

```typescript
| { event: "notify"; level: "info" | "warning" | "error"; message: string }
| { event: "status"; key: string; text: string | null }
```

### Messages Sync Events

For initial load and reconnection.

```typescript
// Full message list (response to get_messages or on reconnect)
| { event: "messages"; messages: Message[] }

// Persisted count (after hstry write)
| { event: "persisted"; message_count: number }
```

---

## Part 3: Canonical Commands (Frontend -> Backend -> Runner)

Commands flow from frontend through backend to the appropriate runner.

### Command Envelope

```typescript
type Command = {
  id?: string;                  // correlation ID for response matching
  session_id: string;           // target session
  runner_id?: string;           // target runner (backend resolves if omitted)
} & CommandPayload;
```

### Session Lifecycle Commands

```typescript
| { cmd: "session.create"; config: SessionConfig }
| { cmd: "session.close" }
| { cmd: "session.new"; parent_session?: string }
| { cmd: "session.switch"; session_path: string }
```

`SessionConfig` specifies the harness and runtime parameters:

```typescript
type SessionConfig = {
  harness: string;               // "pi" | "opencode" | future harnesses
  cwd?: string;                  // working directory
  provider?: string;             // LLM provider hint
  model?: string;                // model ID hint
  continue_session?: string;     // resume from existing session file
};
```

### Agent Commands

```typescript
// Core conversation
| { cmd: "prompt"; message: string; images?: ImageAttachment[] }
| { cmd: "steer"; message: string }
| { cmd: "follow_up"; message: string }
| { cmd: "abort" }

// Input responses (answering agent.input_needed)
| { cmd: "input_response"; request_id: string;
    value?: string; confirmed?: boolean; cancelled?: boolean }
```

### Query Commands

These return data via a response event with matching correlation `id`.

```typescript
| { cmd: "get_state" }
| { cmd: "get_messages" }
| { cmd: "get_stats" }
| { cmd: "get_models" }
| { cmd: "get_commands" }
| { cmd: "get_fork_points" }
```

### Configuration Commands

```typescript
| { cmd: "set_model"; provider: string; model_id: string }
| { cmd: "cycle_model" }
| { cmd: "set_thinking_level"; level: string }
| { cmd: "cycle_thinking_level" }
| { cmd: "set_auto_compaction"; enabled: boolean }
| { cmd: "set_auto_retry"; enabled: boolean }
| { cmd: "compact"; instructions?: string }
| { cmd: "abort_retry" }
| { cmd: "set_session_name"; name: string }
```

### Forking

```typescript
| { cmd: "fork"; entry_id: string }
```

### Command Responses

Every command gets exactly one response:

```typescript
type CommandResponse = {
  id: string;                    // echoed correlation ID
  cmd: string;                   // which command this responds to
  success: boolean;
  data?: unknown;                // command-specific response data
  error?: string;                // on failure
};
```

---

## Part 4: Mux Channel Structure

The WebSocket remains multiplexed. Channels are orthogonal services.

```typescript
type Channel =
  | "agent"      // canonical agent protocol (replaces "pi" and "session")
  | "files"      // file operations (unchanged)
  | "terminal"   // terminal I/O (unchanged)
  | "hstry"      // history queries (unchanged)
  | "trx"        // issue tracking (unchanged)
  | "system";    // connection lifecycle (unchanged)
```

Key changes:
- `"pi"` and `"session"` channels merge into a single `"agent"` channel. The canonical protocol handles pi, opencode, and any future harness.
- mmry stays HTTP-proxied for now. Can be promoted to a WS channel later if needed.

### Harness Selection

When creating a session, the frontend specifies which harness the runner should use:

```typescript
type SessionConfig = {
  harness: "pi" | "opencode" | string;  // which agent harness to run
  cwd?: string;                          // working directory
  provider?: string;                     // LLM provider
  model?: string;                        // model ID
  continue_session?: string;             // resume from session file
};
```

The runner advertises which harnesses it supports at registration time (see Part 5). The frontend uses this to populate the harness picker. If a runner only supports one harness, the UI can hide the picker.

### Agent Channel Wire Format

Commands:
```json
{"channel": "agent", "id": "req-1", "session_id": "ses_abc", "cmd": "prompt", "message": "Hello"}
```

Events:
```json
{"channel": "agent", "session_id": "ses_abc", "runner_id": "local", "ts": 1738764000000, "event": "agent.working", "phase": "generating"}
```

Messages (as events):
```json
{"channel": "agent", "session_id": "ses_abc", "runner_id": "local", "ts": 1738764000000, "event": "stream.text_delta", "message_id": "msg-1", "delta": "Hello", "content_index": 0}
```

---

## Part 5: Runner Protocol

### Transport

Runners communicate with the backend using newline-delimited JSON-RPC over a persistent bidirectional stream:

| Deployment | Transport | Notes |
|-----------|-----------|-------|
| Local (same host) | Unix socket at `/run/user/{uid}/octo-runner.sock` | Existing pattern |
| Remote (VM/workstation) | WebSocket over TLS | Runner connects outward to backend |

The protocol is the same regardless of transport: one JSON object per line, each with a `type` field for routing.

### Runner Registration

When a runner connects to the backend:

```json
{"type": "runner.hello", "runner_id": "wkst-alice-01", "hostname": "alice-workstation",
 "harnesses": ["pi", "opencode"], "max_sessions": 10,
 "version": "0.1.0", "os": "linux"}
```

- `harnesses` -- which agent harnesses this runner can spawn. The backend exposes this to the frontend so users can pick.

Backend acknowledges:

```json
{"type": "runner.welcome", "runner_id": "wkst-alice-01"}
```

### Backend -> Runner (Commands)

Canonical commands forwarded with routing info:

```json
{"type": "command", "session_id": "ses_abc", "user_id": "alice",
 "cmd": "session.create", "config": {"harness": "pi", "cwd": "/home/alice/project"},
 "id": "req-1"}
```

```json
{"type": "command", "session_id": "ses_abc", "user_id": "alice",
 "cmd": "prompt", "message": "Hello", "id": "req-2"}
```

### Runner -> Backend (Events + Responses)

Canonical events and command responses:

```json
{"type": "event", "session_id": "ses_abc", "event": "agent.working",
 "phase": "generating", "ts": 1738764000000}
```

```json
{"type": "response", "id": "req-2", "cmd": "prompt", "success": true}
```

### Runner Heartbeat

The runner sends periodic heartbeats with per-session process telemetry:

```json
{"type": "runner.heartbeat", "runner_id": "wkst-alice-01",
 "uptime_s": 3600,
 "sessions": [
   {"session_id": "ses_abc", "harness": "pi",
    "process": {"alive": true, "pid": 12345, "rss_bytes": 104857600,
                "cpu_pct": 2.3, "uptime_s": 1200}}
 ]}
```

If the backend doesn't receive a heartbeat within the configured timeout (default 30s), it marks the runner as unhealthy.

### Process-Level Monitoring

The runner owns the agent process (it spawned it, it has the PID). This is the primary health mechanism -- more reliable than any in-protocol ping:

**Continuous monitoring (every 2s per session):**
- Read `/proc/{pid}/stat` (Linux) or equivalent -- alive, RSS, CPU
- Check stdout pipe is open (not broken)

**Immediate crash detection:**
- Child process exit handler fires instantly on unexpected exit
- Runner immediately emits: `{"type": "event", "session_id": "ses_abc", "event": "agent.error", "error": "Process exited with code 1", "recoverable": false}`
- Stderr contents included in the error if available

**Hang detection:**
- If the session is in `working` state and no stdout output for > 30s, runner emits a warning event
- If > 60s, runner can optionally kill and restart the process

**Protocol-level health check (supplementary):**
- Runner sends `get_state` to the agent's stdin with a 5s timeout
- If no response, the process is considered unresponsive even if `/proc/{pid}` says it's alive
- Used on a slower cadence (every 30s) as a deeper liveness check

**Graceful shutdown:**
- Runner sends SIGTERM to the agent PID
- Waits up to 3s for the process to exit
- If still alive, sends SIGKILL
- Emits `session.closed` event

This means we do NOT need a `ping` or `shutdown` command in the agent harness protocol. The runner handles both concerns using standard process management.

---

## Part 6: Pi Agent Mapping

How pi's native RPC events map to the canonical protocol.

### Pi -> Canonical Event Mapping

| Pi Event | Canonical Event |
|----------|-----------------|
| `agent_start` | `agent.working { phase: "generating" }` |
| `agent_end` | `agent.idle` |
| `turn_start` | (no direct mapping -- absorbed into working state) |
| `turn_end` | (no direct mapping) |
| `message_start` | `stream.message_start { message_id, role }` |
| `message_update` + `text_delta` | `stream.text_delta { delta }` |
| `message_update` + `thinking_delta` | `stream.thinking_delta { delta }` |
| `message_update` + `toolcall_start` | `stream.tool_call_start { tool_call_id, name }` |
| `message_update` + `toolcall_delta` | `stream.tool_call_delta { delta }` |
| `message_update` + `toolcall_end` | `stream.tool_call_end { tool_call }` |
| `message_update` + `done` | `stream.done { reason }` |
| `message_update` + `error` | `agent.error { error, recoverable }` |
| `message_end` | `stream.message_end { message }` (with full canonical Message) |
| `tool_execution_start` | `tool.start { tool_call_id, name, input }` + `agent.working { phase: "tool_running", detail: name }` |
| `tool_execution_update` | `tool.progress { partial_output }` |
| `tool_execution_end` | `tool.end { output, is_error, duration_ms }` |
| `auto_compaction_start` | `compact.start { reason }` + `agent.working { phase: "compacting" }` |
| `auto_compaction_end` | `compact.end { success, will_retry }` |
| `auto_retry_start` | `retry.start { attempt, max_attempts, delay_ms, error }` + `agent.working { phase: "retrying" }` |
| `auto_retry_end` | `retry.end { success, attempt }` |
| `extension_ui_request` (dialog) | `agent.input_needed { request }` |
| `extension_ui_request` (notify) | `notify { level, message }` |
| `extension_ui_request` (setStatus) | `status { key, text }` |

### Pi Message -> Canonical Message Mapping

Pi's `AgentMessage` maps to canonical `Message`:

| Pi Field | Canonical Field |
|----------|-----------------|
| `role` | `role` (map "toolResult" -> "tool") |
| `content` (array) | `parts` (map each content block to Part) |
| `timestamp` | `created_at` |
| `model` | `model` |
| `provider` | `provider` |
| `usage` | `usage` (map field names) |
| `stopReason` | `stop_reason` |
| `toolCallId` | `tool_call_id` |
| `toolName` | `tool_name` |
| `isError` | `is_error` |

Pi content blocks map to Parts:
- `{ type: "text", text }` -> `{ type: "text", id: gen(), text }`
- `{ type: "thinking", thinking }` -> `{ type: "thinking", id: gen(), text: thinking }`
- `{ type: "toolCall", id, name, arguments }` -> `{ type: "tool_call", id: gen(), tool_call_id: id, name, input: arguments, status }`
- `{ type: "image", source }` -> `{ type: "image", id: gen(), source }`

---

## Part 7: What Pi Needs (Extension Only -- No Core Changes)

The pi core is untouched. Everything is achieved through a single pi extension
(`octo-bridge.ts`) combined with the runner's process-level monitoring.

### Responsibility Split

| Concern | Who | How |
|---------|-----|-----|
| Health/ping | Runner | PID monitoring + `get_state` with timeout |
| Shutdown | Runner | SIGTERM/SIGKILL on PID |
| Queue depth | Runner | Polls `get_state` -> `pendingMessageCount` |
| Phase tracking | Extension + Runner | Extension emits `setStatus`, runner interprets |
| Crash detection | Runner | Child process exit handler on PID |
| Memory/CPU | Runner | `/proc/{pid}/stat` |

### The `octo-bridge` Extension

A single pi extension that emits granular phase information via `ctx.ui.setStatus()`.
In RPC mode, these become `extension_ui_request` events with `method: "setStatus"`
on stdout, which the runner reads and translates to canonical `agent.working` events.

```typescript
// extensions/octo-bridge.ts
import type { ExtensionAPI, ExtensionContext } from "pi";

export default function octoBridge(pi: ExtensionAPI) {
  const status = (ctx: ExtensionContext, phase: string, detail?: string) => {
    ctx.ui.setStatus("octo_phase", detail ? `${phase}:${detail}` : phase);
  };

  const clear = (ctx: ExtensionContext) => {
    ctx.ui.setStatus("octo_phase", undefined);
  };

  // --- Phase: generating ---
  pi.on("agent_start", (_event, ctx) => {
    status(ctx, "generating");
  });

  pi.on("agent_end", (_event, ctx) => {
    clear(ctx);
  });

  // --- Phase: thinking ---
  // Detected by runner from thinking_delta events (no extension hook needed)

  // --- Phase: tool_running ---
  pi.on("tool_call", (event, ctx) => {
    status(ctx, "tool_running", event.toolName);
    // Don't block -- return undefined to let tool execute
  });

  pi.on("tool_result", (event, ctx) => {
    // Tool done -- back to generating (next turn will start if needed)
    status(ctx, "generating");
  });

  // --- Phase: compacting ---
  pi.on("session_before_compact", (_event, ctx) => {
    status(ctx, "compacting");
    // Don't cancel -- return undefined
  });

  pi.on("session_compact", (_event, ctx) => {
    // Compaction done -- let runner figure out next state from
    // subsequent agent events
    clear(ctx);
  });

  // --- Queue tracking ---
  // The runner polls get_state.pendingMessageCount for this.
  // No extension hook needed -- pi already tracks it internally.

  // --- Session name auto-generation ---
  // (existing octo extension logic can live here too)
}
```

### What the Runner Sees

When the extension emits `setStatus("octo_phase", "tool_running:bash")`,
pi's RPC mode outputs:

```json
{"type": "extension_ui_request", "id": "ext-1", "method": "setStatus",
 "statusKey": "octo_phase", "statusText": "tool_running:bash"}
```

The runner parses this and emits the canonical event:

```json
{"type": "event", "session_id": "ses_abc",
 "event": "agent.working", "phase": "tool_running", "detail": "bash",
 "ts": 1738764000000}
```

When the extension emits `setStatus("octo_phase", undefined)` (clear),
the runner does NOT emit `agent.idle` immediately -- it waits for `agent_end`
from pi's native event stream, since the agent may still be working
(e.g., transitioning from tool result to next LLM turn).

### Runner's Event Translation Logic

The runner maintains a simple state machine per session:

```
State: idle | working(phase)

Transitions:
  pi agent_start          -> working("generating")
  pi agent_end            -> idle
  setStatus(octo_phase=X) -> working(parse(X))    [only if already working]
  setStatus(octo_phase=)  -> working("generating") [fall back, don't go idle]
  pi auto_retry_start     -> working("retrying")
  pi auto_compaction_start-> working("compacting")
  pi tool_execution_start -> working("tool_running")
  process exit            -> emit agent.error, then idle
```

The key rule: **`agent_start` and `agent_end` are the authoritative
idle/working transitions.** The `setStatus` events from the extension
only refine the phase WITHIN the working state. The runner never
transitions to idle based on extension status -- only on `agent_end`
or process death.

### What We Avoid

By not touching pi core:
- No fork to maintain
- Pi upgrades are drop-in (just update the binary)
- Extension is a single file shipped alongside the runner
- If the extension fails, pi still works -- we just get less granular
  phase info (runner falls back to deriving phase from native events only)

---

## Part 8: Two-Stage Delegation Routing

Delegation between agents uses a two-stage routing model. The runner handles
local delegation directly; the backend handles remote/cross-runner delegation.
This means agents can communicate locally even without the backend running.

### Why Two Stages?

1. **Offline local collaboration.** A runner managing multiple sessions on one
   machine (e.g., Pi + Claude Code) can route messages between them with zero
   network hops. The runner's event bus is sufficient.

2. **Latency.** Local delegation avoids the round-trip through the backend's
   WebSocket layer. For tight tool-calling loops between agents, this matters.

3. **Simplicity.** The runner already owns the session processes and their
   stdin/stdout pipes. It can inject a delegated prompt directly.

4. **Resilience.** If the backend goes down, local agent collaboration continues.
   The runner queues delegation events for the backend on reconnect.

### Routing Decision

When the runner receives a `Delegate` command (either from the backend or from
an agent's tool call), it checks whether the target session is local:

```
Runner receives Delegate { target_session_id, target_runner_id? }
  |
  |--> target_session_id is managed by THIS runner?
  |      |
  |      YES --> Route locally (inject prompt, stream response back)
  |      |
  |      NO  --> Is target_runner_id == this runner?
  |               |
  |               YES --> Error: session not found
  |               NO  --> Forward to backend (escalate)
```

### Local Delegation Flow

```
Session A (on Runner R)          Runner R            Session B (on Runner R)
    |                                |                        |
    |-- delegate(target=B) --------->|                        |
    |                                |-- prompt(msg) -------->|
    |<-- delegate.start -------------|                        |
    |                                |<-- stream events ------|
    |<-- delegate.delta -------------|                        |
    |<-- delegate.delta -------------|                        |
    |                                |<-- stream.done --------|
    |<-- delegate.end ---------------|                        |
```

The runner acts as the delegation coordinator. It:
1. Validates the request (permission checks use the same `DelegationPermission` rules)
2. Injects the prompt into Session B's stdin (with sender identity: `[pi:ses_A]: message`)
3. Streams Session B's response events back to Session A as `delegate.delta` events
4. Emits `delegate.end` when Session B finishes

### Remote Delegation Flow (via Backend)

When the target is on a different runner, the runner escalates to the backend:

```
Session A       Runner A          Backend           Runner B        Session B
    |               |                |                  |                |
    |-- delegate -->|                |                  |                |
    |               |-- escalate --->|                  |                |
    |               |                |-- command ------>|                |
    |               |                |                  |-- prompt ----->|
    |<-- start -----|<-- start ------|                  |                |
    |               |                |<-- events -------|<-- stream -----|
    |<-- delta -----|<-- delta ------|                  |                |
    |<-- end -------|<-- end --------|                  |                |
```

The escalation message from runner to backend is:

```json
{"type": "delegate.escalate", "session_id": "ses_A",
 "request": { ... DelegateRequest ... }}
```

The backend then routes using its runner registry, forwarding the delegation
as a `Delegate` command to Runner B. Events flow back through the backend
to Runner A, which delivers them to Session A.

### Runner-to-Backend Escalation Protocol

New wire message for the runner-to-backend direction:

```typescript
// Runner -> Backend: "I can't handle this delegation, please route it"
| { type: "delegate.escalate";
    session_id: string;          // originating session
    request: DelegateRequest;    // the full delegation request
    correlation_id: string }     // for matching the response stream
```

The backend responds with delegation events (`delegate.start`, `delegate.delta`,
`delegate.end` / `delegate.error`) targeted at the originating session, which
the runner forwards to Session A.

### Permission Enforcement

Both runner and backend enforce `DelegationPermission` rules:

| Check | Runner (local) | Backend (remote) |
|-------|----------------|------------------|
| Source/target pattern match | Yes | Yes |
| Max depth | Yes | Yes |
| Sandbox profile | Yes | Yes |
| Cross-runner allowed | N/A | Yes |
| Rate limiting | Optional | Yes |

The runner loads permissions from its config (synced from backend on connect).
For local delegation, the runner is the sole authority. For remote delegation,
the backend re-validates (defense in depth).

### Agent-Initiated Delegation

Agents can delegate via:

1. **`delegate` tool call** -- The agent calls a tool named `delegate` with
   `{target_session_id, message}`. The runner intercepts this tool call,
   performs the delegation, and returns the result as a tool result.

2. **`octo-delegate` CLI** -- The agent runs `octo-delegate --target ses_xyz "message"`.
   This CLI talks to the runner's local socket. Same routing logic applies.

3. **`@@agent:session` syntax** -- User types `@@pi:ses_xyz do X` in chat.
   The frontend sends this as a `Delegate` command.

All three paths converge at the runner's delegation handler.

---

## Part 9: Migration Path

### Phase 1: Define Types (this document + code)
- Canonical Rust types in a shared crate (`octo-protocol` or similar)
- Canonical TypeScript types in `frontend/lib/canonical-types.ts`
- Both kept in sync manually (Rust is source of truth)

### Phase 2: Build `octo-bridge` Pi Extension
- Single TS file: `extensions/octo-bridge.ts`
- Emits `setStatus("octo_phase", ...)` for phase tracking
- Shipped alongside the runner, loaded via `--extension` flag when spawning pi
- No pi core changes required

### Phase 3: Runner Produces Canonical Format
- Runner's stdout reader parses pi native events
- Runner reads `setStatus` events from the extension
- Runner maintains per-session state machine (idle/working)
- Runner emits canonical events to the backend
- Runner handles process monitoring via PID

### Phase 4: Backend Pass-Through
- Backend receives canonical events from runner, forwards to WebSocket unchanged
- Backend receives canonical commands from WebSocket, forwards to runner unchanged
- Backend routes commands to the correct runner based on session -> runner mapping
- Existing pi-specific translation layer removed

### Phase 5: Frontend Migration
- Replace `useChat.ts` event handling with canonical event consumer
- Remove all pi-specific state reconstruction logic (isStreaming, isAwaitingResponse, sendPending)
- The state machine becomes trivial: subscribe to events, render current state
- Single `useAgent` hook replaces `useChat`

### Phase 6: hstry Alignment
- hstry stores canonical `Message` directly (already 95% aligned)
- Runner persists to hstry using canonical format on `agent_end`
- Frontend reads from hstry in canonical format (no translation)

### Phase 7: Multi-Runner Support
- Frontend shows runner selector when creating sessions
- Frontend shows harness selector based on runner capabilities
- Backend manages runner connections (local unix socket + remote WebSocket)
- Sessions are addressable by `(runner_id, session_id)`
