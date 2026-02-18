# Pi Session Architecture Refactor

## Overview

Move Pi session management from oqto backend to oqto-runner. This enables:
- Single multiplexed WebSocket for all services (pi, files, terminal, hstry)
- Runner owns Pi process lifecycle and hstry writes
- Backend becomes stateless relay
- Clean user isolation (one runner per user)

## Architecture

```
Frontend                          Backend                           Runner (per user)
   |                                 |                                    |
   |-- Single WebSocket ------------>|                                    |
   |   (multiplexed channels)        |                                    |
   |                                 |-- Unix/TCP socket ---------------->|
   |                                 |   (runner protocol)                |
   |                                 |                                    |
   |   {channel:"pi", ...}           |   PiCommand::Prompt{...}          |-- Pi Process A
   |   {channel:"files", ...}        |   PiCommand::Subscribe{...}       |-- Pi Process B
   |   {channel:"terminal", ...}     |   ...                             |-- hstry (direct)
   |                                 |                                    |
```

---

## Interface Contracts

### 1. WebSocket Protocol (Frontend <-> Backend)

All messages include a `channel` field for routing.

#### Channels
- `pi` - Pi session commands and events
- `files` - File operations (read, write, list, stat)
- `terminal` - Terminal I/O
- `hstry` - History queries

#### Message Format

```typescript
// Frontend -> Backend
type WsCommand = {
  id?: string;           // Optional request ID for correlation
  channel: Channel;
  // Channel-specific fields...
};

// Backend -> Frontend
type WsEvent = {
  id?: string;           // Correlation ID (if responding to command)
  channel: Channel;
  // Channel-specific fields...
};

type Channel = "pi" | "files" | "terminal" | "hstry" | "system";
```

#### Pi Channel Commands (Frontend -> Backend)

```typescript
// === Session Lifecycle ===
{ channel: "pi", type: "create_session", session_id: string, config?: PiSessionConfig }
{ channel: "pi", type: "close_session", session_id: string }
{ channel: "pi", type: "new_session", session_id: string, parent_session?: string }
{ channel: "pi", type: "switch_session", session_id: string, session_path: string }
{ channel: "pi", type: "list_sessions" }
{ channel: "pi", type: "subscribe", session_id: string }
{ channel: "pi", type: "unsubscribe", session_id: string }

// === Prompting ===
{ channel: "pi", type: "prompt", session_id: string, message: string, images?: ImageContent[] }
{ channel: "pi", type: "steer", session_id: string, message: string }
{ channel: "pi", type: "follow_up", session_id: string, message: string }
{ channel: "pi", type: "abort", session_id: string }

// === State & Messages ===
{ channel: "pi", type: "get_state", session_id: string }
{ channel: "pi", type: "get_messages", session_id: string }
{ channel: "pi", type: "get_session_stats", session_id: string }
{ channel: "pi", type: "get_last_assistant_text", session_id: string }

// === Model Management ===
{ channel: "pi", type: "set_model", session_id: string, provider: string, model_id: string }
{ channel: "pi", type: "cycle_model", session_id: string }
{ channel: "pi", type: "get_available_models", session_id: string }

// === Thinking Level ===
{ channel: "pi", type: "set_thinking_level", session_id: string, level: ThinkingLevel }
{ channel: "pi", type: "cycle_thinking_level", session_id: string }

// === Compaction ===
{ channel: "pi", type: "compact", session_id: string, instructions?: string }
{ channel: "pi", type: "set_auto_compaction", session_id: string, enabled: boolean }

// === Queue Modes ===
{ channel: "pi", type: "set_steering_mode", session_id: string, mode: "all" | "one-at-a-time" }
{ channel: "pi", type: "set_follow_up_mode", session_id: string, mode: "all" | "one-at-a-time" }

// === Retry ===
{ channel: "pi", type: "set_auto_retry", session_id: string, enabled: boolean }
{ channel: "pi", type: "abort_retry", session_id: string }

// === Forking ===
{ channel: "pi", type: "fork", session_id: string, entry_id: string }
{ channel: "pi", type: "get_fork_messages", session_id: string }

// === Session Metadata ===
{ channel: "pi", type: "set_session_name", session_id: string, name: string }
{ channel: "pi", type: "export_html", session_id: string, output_path?: string }

// === Commands/Skills ===
{ channel: "pi", type: "get_commands", session_id: string }

// === Bash (user-initiated) ===
{ channel: "pi", type: "bash", session_id: string, command: string }
{ channel: "pi", type: "abort_bash", session_id: string }

// === Extension UI Responses ===
{ channel: "pi", type: "extension_ui_response", session_id: string, id: string, value?: string, confirmed?: boolean, cancelled?: boolean }
```

#### Types

```typescript
type ThinkingLevel = "off" | "minimal" | "low" | "medium" | "high" | "xhigh";

type ImageContent = {
  type: "image";
  source: { type: "base64"; mediaType: string; data: string } | { type: "url"; url: string };
};

type PiSessionConfig = {
  cwd: string;
  provider?: string;
  model?: string;
  continue_session?: string;
  system_prompt_files?: string[];
  env?: Record<string, string>;
};
```

#### Pi Channel Events (Backend -> Frontend)

```typescript
// === Session Lifecycle Events ===
{ channel: "pi", type: "session_created", session_id: string }
{ channel: "pi", type: "session_closed", session_id: string }
{ channel: "pi", type: "sessions", sessions: PiSessionInfo[] }

// === State Events ===
{ channel: "pi", type: "state", session_id: string, state: PiState }
{ channel: "pi", type: "messages", session_id: string, messages: AgentMessage[] }
{ channel: "pi", type: "session_stats", session_id: string, stats: SessionStats }

// === Model Events ===
{ channel: "pi", type: "model_changed", session_id: string, model: PiModel, thinking_level: ThinkingLevel, is_scoped: boolean }
{ channel: "pi", type: "available_models", session_id: string, models: PiModel[] }
{ channel: "pi", type: "thinking_level_changed", session_id: string, level: ThinkingLevel }

// === Streaming Events ===
{ channel: "pi", type: "agent_start", session_id: string }
{ channel: "pi", type: "agent_end", session_id: string, messages: AgentMessage[] }
{ channel: "pi", type: "turn_start", session_id: string }
{ channel: "pi", type: "turn_end", session_id: string, message: AgentMessage, tool_results: ToolResultMessage[] }
{ channel: "pi", type: "message_start", session_id: string, message: AgentMessage }
{ channel: "pi", type: "message_update", session_id: string, message: AgentMessage, delta: AssistantMessageDelta }
{ channel: "pi", type: "message_end", session_id: string, message: AgentMessage }

// === Tool Events ===
{ channel: "pi", type: "tool_execution_start", session_id: string, tool_call_id: string, tool_name: string, args: any }
{ channel: "pi", type: "tool_execution_update", session_id: string, tool_call_id: string, tool_name: string, partial_result: ToolResult }
{ channel: "pi", type: "tool_execution_end", session_id: string, tool_call_id: string, tool_name: string, result: ToolResult, is_error: boolean }

// === Compaction Events ===
{ channel: "pi", type: "auto_compaction_start", session_id: string, reason: "threshold" | "overflow" }
{ channel: "pi", type: "auto_compaction_end", session_id: string, result?: CompactionResult, aborted: boolean, will_retry: boolean }
{ channel: "pi", type: "compaction_result", session_id: string, result: CompactionResult }

// === Retry Events ===
{ channel: "pi", type: "auto_retry_start", session_id: string, attempt: number, max_attempts: number, delay_ms: number, error_message: string }
{ channel: "pi", type: "auto_retry_end", session_id: string, success: boolean, attempt: number, final_error?: string }

// === Fork Events ===
{ channel: "pi", type: "fork_messages", session_id: string, messages: ForkMessage[] }
{ channel: "pi", type: "fork_result", session_id: string, text: string, cancelled: boolean }

// === Commands/Skills Events ===
{ channel: "pi", type: "commands", session_id: string, commands: CommandInfo[] }

// === Bash Events ===
{ channel: "pi", type: "bash_result", session_id: string, output: string, exit_code: number, cancelled: boolean, truncated: boolean, full_output_path?: string }

// === Extension UI Events ===
{ channel: "pi", type: "extension_ui_request", session_id: string, id: string, method: string, ...method_specific_fields }
{ channel: "pi", type: "extension_error", session_id: string, extension_path: string, event: string, error: string }

// === Error Events ===
{ channel: "pi", type: "error", session_id: string, error: string }

// === Persistence Events ===
{ channel: "pi", type: "persisted", session_id: string, message_count: number }
```

#### Event Types

```typescript
type AssistantMessageDelta = 
  | { type: "start"; partial: any }
  | { type: "text_start"; content_index: number; partial: any }
  | { type: "text_delta"; content_index: number; delta: string; partial: any }
  | { type: "text_end"; content_index: number; content: string; partial: any }
  | { type: "thinking_start"; content_index: number; partial: any }
  | { type: "thinking_delta"; content_index: number; delta: string; partial: any }
  | { type: "thinking_end"; content_index: number; content: string; partial: any }
  | { type: "toolcall_start"; content_index: number; partial: any }
  | { type: "toolcall_delta"; content_index: number; delta: string; partial: any }
  | { type: "toolcall_end"; content_index: number; tool_call: ToolCall; partial: any }
  | { type: "done"; reason: "stop" | "length" | "toolUse"; message?: AgentMessage }
  | { type: "error"; reason: "aborted" | "error"; error?: any };

type ForkMessage = { entry_id: string; text: string };

type CommandInfo = {
  name: string;
  description?: string;
  source: "extension" | "template" | "skill";
  location?: "user" | "project" | "path";
  path?: string;
};

type CompactionResult = {
  summary: string;
  first_kept_entry_id: string;
  tokens_before: number;
  details?: any;
};
```

#### System Channel Events

```typescript
// Connection established
{ channel: "system", type: "connected" }

// Error not tied to specific channel
{ channel: "system", type: "error", error: string }
```

---

### 2. Runner Protocol (Backend <-> Runner)

Extends existing `RunnerRequest`/`RunnerResponse` enums.

#### New Request Types

```rust
// In backend/crates/oqto/src/runner/protocol.rs

// === Session Lifecycle ===
PiCreateSession(PiCreateSessionRequest),
PiCloseSession(PiCloseSessionRequest),
PiNewSession(PiNewSessionRequest),
PiSwitchSession(PiSwitchSessionRequest),
PiListSessions,
PiSubscribe(PiSubscribeRequest),
PiUnsubscribe(PiUnsubscribeRequest),

// === Prompting ===
PiPrompt(PiPromptRequest),
PiSteer(PiSteerRequest),
PiFollowUp(PiFollowUpRequest),
PiAbort(PiAbortRequest),

// === State & Messages ===
PiGetState(PiGetStateRequest),
PiGetMessages(PiGetMessagesRequest),
PiGetSessionStats(PiGetSessionStatsRequest),
PiGetLastAssistantText(PiGetLastAssistantTextRequest),

// === Model Management ===
PiSetModel(PiSetModelRequest),
PiCycleModel(PiCycleModelRequest),
PiGetAvailableModels(PiGetAvailableModelsRequest),

// === Thinking Level ===
PiSetThinkingLevel(PiSetThinkingLevelRequest),
PiCycleThinkingLevel(PiCycleThinkingLevelRequest),

// === Compaction ===
PiCompact(PiCompactRequest),
PiSetAutoCompaction(PiSetAutoCompactionRequest),

// === Queue Modes ===
PiSetSteeringMode(PiSetSteeringModeRequest),
PiSetFollowUpMode(PiSetFollowUpModeRequest),

// === Retry ===
PiSetAutoRetry(PiSetAutoRetryRequest),
PiAbortRetry(PiAbortRetryRequest),

// === Forking ===
PiFork(PiForkRequest),
PiGetForkMessages(PiGetForkMessagesRequest),

// === Session Metadata ===
PiSetSessionName(PiSetSessionNameRequest),
PiExportHtml(PiExportHtmlRequest),

// === Commands/Skills ===
PiGetCommands(PiGetCommandsRequest),

// === Bash ===
PiBash(PiBashRequest),
PiAbortBash(PiAbortBashRequest),

// === Extension UI ===
PiExtensionUiResponse(PiExtensionUiResponseRequest),
```

#### Request Structs

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionConfig {
    pub cwd: PathBuf,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub continue_session: Option<PathBuf>,
    pub system_prompt_files: Vec<PathBuf>,
    pub env: HashMap<String, String>,
}

// Session Lifecycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiCreateSessionRequest { pub session_id: String, pub config: PiSessionConfig }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiCloseSessionRequest { pub session_id: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiNewSessionRequest { pub session_id: String, pub parent_session: Option<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSwitchSessionRequest { pub session_id: String, pub session_path: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSubscribeRequest { pub session_id: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiUnsubscribeRequest { pub session_id: String }

// Prompting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiPromptRequest { pub session_id: String, pub message: String, pub images: Option<Vec<ImageContent>> }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSteerRequest { pub session_id: String, pub message: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiFollowUpRequest { pub session_id: String, pub message: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiAbortRequest { pub session_id: String }

// State & Messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiGetStateRequest { pub session_id: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiGetMessagesRequest { pub session_id: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiGetSessionStatsRequest { pub session_id: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiGetLastAssistantTextRequest { pub session_id: String }

// Model Management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSetModelRequest { pub session_id: String, pub provider: String, pub model_id: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiCycleModelRequest { pub session_id: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiGetAvailableModelsRequest { pub session_id: String }

// Thinking Level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSetThinkingLevelRequest { pub session_id: String, pub level: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiCycleThinkingLevelRequest { pub session_id: String }

// Compaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiCompactRequest { pub session_id: String, pub instructions: Option<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSetAutoCompactionRequest { pub session_id: String, pub enabled: bool }

// Queue Modes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSetSteeringModeRequest { pub session_id: String, pub mode: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSetFollowUpModeRequest { pub session_id: String, pub mode: String }

// Retry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSetAutoRetryRequest { pub session_id: String, pub enabled: bool }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiAbortRetryRequest { pub session_id: String }

// Forking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiForkRequest { pub session_id: String, pub entry_id: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiGetForkMessagesRequest { pub session_id: String }

// Session Metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSetSessionNameRequest { pub session_id: String, pub name: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiExportHtmlRequest { pub session_id: String, pub output_path: Option<String> }

// Commands/Skills
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiGetCommandsRequest { pub session_id: String }

// Bash
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiBashRequest { pub session_id: String, pub command: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiAbortBashRequest { pub session_id: String }

// Extension UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiExtensionUiResponseRequest {
    pub session_id: String,
    pub id: String,
    pub value: Option<String>,
    pub confirmed: Option<bool>,
    pub cancelled: Option<bool>,
}
```

#### New Response Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionCreatedResponse {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionInfo {
    pub session_id: String,
    pub state: PiSessionState,
    pub last_activity: i64,  // Unix timestamp ms
    pub subscriber_count: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PiSessionState {
    Starting,
    Idle,
    Streaming,
    Compacting,
    Stopping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionListResponse {
    pub sessions: Vec<PiSessionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiStateResponse {
    pub session_id: String,
    pub state: PiState,  // Reuse existing PiState from pi/types.rs
}

// Add to RunnerResponse enum:
//   PiSessionCreated(PiSessionCreatedResponse),
//   PiSessionList(PiSessionListResponse),
//   PiState(PiStateResponse),
//   PiSessionClosed { session_id: String },
//   PiEvent(PiEventWrapper),  // Streamed events
```

#### Event Streaming

Runner streams Pi events to backend via the existing stdout subscription mechanism,
but wrapped with session context:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiEventWrapper {
    pub session_id: String,
    pub event: PiEvent,  // From pi/types.rs
}

// Sent as RunnerResponse::PiEvent(PiEventWrapper)
```

---

### 3. Runner Internal: Pi Session Manager

```rust
// In oqto-runner

use std::collections::HashMap;
use tokio::process::{Child, ChildStdin};
use tokio::sync::{broadcast, mpsc, RwLock};

pub struct PiSessionManager {
    sessions: RwLock<HashMap<String, PiSession>>,
    config: PiManagerConfig,
}

struct PiSession {
    id: String,
    config: PiSessionConfig,
    process: Child,
    stdin: ChildStdin,
    state: PiSessionState,
    last_activity: std::time::Instant,
    /// Broadcast channel for events - subscribers receive cloned events
    event_tx: broadcast::Sender<PiEvent>,
    /// Command sender to the session task
    cmd_tx: mpsc::Sender<PiSessionCommand>,
}

enum PiSessionCommand {
    Prompt(String),
    Steer(String),
    FollowUp(String),
    Abort,
    Compact(Option<String>),
    GetState(oneshot::Sender<PiState>),
    Close,
}

impl PiSessionManager {
    pub async fn create_session(&self, id: String, config: PiSessionConfig) -> Result<()>;
    pub async fn get_or_create_session(&self, id: &str, config: PiSessionConfig) -> Result<()>;
    pub async fn prompt(&self, session_id: &str, message: &str) -> Result<()>;
    pub async fn steer(&self, session_id: &str, message: &str) -> Result<()>;
    pub async fn follow_up(&self, session_id: &str, message: &str) -> Result<()>;
    pub async fn abort(&self, session_id: &str) -> Result<()>;
    pub async fn compact(&self, session_id: &str, instructions: Option<&str>) -> Result<()>;
    pub async fn subscribe(&self, session_id: &str) -> Result<broadcast::Receiver<PiEvent>>;
    pub async fn unsubscribe(&self, session_id: &str) -> Result<()>;
    pub async fn list_sessions(&self) -> Vec<PiSessionInfo>;
    pub async fn get_state(&self, session_id: &str) -> Result<PiState>;
    pub async fn close_session(&self, session_id: &str) -> Result<()>;
    
    /// Background task: cleanup idle sessions
    pub async fn cleanup_loop(&self);
    
    /// Background task: persist to hstry on AgentEnd
    async fn persist_to_hstry(&self, session_id: &str, messages: &[AgentMessage]) -> Result<()>;
}
```

---

## Work Streams

### Stream 1: Runner Pi Session Manager
**Owner:** Agent A

Tasks:
1. Create `PiSessionManager` struct in oqto-runner
2. Implement Pi process spawning with stdin/stdout handling
3. Implement session map (create, get, close, list)
4. Implement command routing (prompt, steer, abort, compact)
5. Implement event broadcast to subscribers
6. Implement state tracking (Starting, Idle, Streaming, etc.)
7. Implement idle cleanup loop
8. Implement direct hstry writes on AgentEnd

**Interface in:** `PiSessionCommand` enum
**Interface out:** `broadcast::Receiver<PiEvent>`

### Stream 2: Multiplexed WebSocket Protocol
**Owner:** Agent B

Tasks:
1. Define TypeScript types for multiplexed WS protocol
2. Create backend `MultiplexedWsHandler`
3. Implement channel routing (pi, files, terminal, hstry)
4. Create frontend `WsConnectionManager` class
5. Implement channel subscription/unsubscription
6. Update `usePiChat` hooks to use new manager
7. Handle reconnection with resubscription

**Interface in:** `WsCommand` from frontend
**Interface out:** `WsEvent` to frontend

### Stream 3: Backend <-> Runner Protocol
**Owner:** Agent C

Tasks:
1. Add Pi request/response types to `protocol.rs`
2. Implement `RunnerClient` methods for Pi operations
3. Implement backend Pi channel handler (routes to runner)
4. Handle event streaming from runner to backend
5. Implement auth for TCP transport (PSK over TLS)
6. Add transport abstraction (Unix socket vs TCP)

**Interface in:** `RunnerRequest::Pi*` variants
**Interface out:** `RunnerResponse::Pi*` variants

---

## Integration Points

After all streams complete:

1. Backend `MultiplexedWsHandler` calls `RunnerClient` Pi methods
2. `RunnerClient` sends `PiCreateSession`, `PiPrompt`, etc. to runner
3. Runner's `PiSessionManager` handles requests
4. Runner streams `PiEvent` back via `RunnerResponse::PiEvent`
5. Backend forwards events to WebSocket as `{channel: "pi", ...}`
6. Frontend `WsConnectionManager` routes to session-specific handlers

---

## Migration Plan

1. **Phase 1:** Implement new components in parallel (3 streams)
2. **Phase 2:** Integration testing with new WS endpoint
3. **Phase 3:** Add feature flag to switch between old/new
4. **Phase 4:** Migrate frontend to new endpoint
5. **Phase 5:** Remove old `MainChatPiService` and per-session WS

---

## Files to Create

```
backend/crates/oqto/src/api/ws_multiplexed.rs      # Stream 2: New WS handler
backend/crates/oqto/src/runner/pi_client.rs        # Stream 3: Runner Pi methods
backend/crates/oqto-runner/src/pi_manager.rs       # Stream 1: Session manager

frontend/lib/ws-manager.ts                          # Stream 2: Connection manager
frontend/features/main-chat/hooks/usePiChatV2.ts   # Stream 2: Updated hooks
```

## Files to Modify

```
backend/crates/oqto/src/runner/protocol.rs         # Stream 3: Add Pi types
backend/crates/oqto/src/bin/oqto-runner.rs         # Stream 1: Integrate manager
backend/crates/oqto/src/api/routes.rs              # Stream 2: Add new WS route
```

## Files to Eventually Remove

```
backend/crates/oqto/src/main_chat/pi_service.rs    # Replaced by runner
backend/crates/oqto/src/api/main_chat_pi.rs        # Old WS handlers
backend/crates/oqto/src/pi/runtime.rs              # No longer needed
frontend/features/main-chat/hooks/cache.ts         # Global WS cache
```
