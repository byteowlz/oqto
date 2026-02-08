//! Runner RPC protocol types.
//!
//! Defines the request/response types for communication between octo and the runner daemon.
//! The protocol uses JSON over Unix sockets with newline-delimited messages.
//!
//! ## Protocol Categories
//!
//! ### Process Management (original)
//! - SpawnProcess, SpawnRpcProcess, KillProcess, GetStatus, ListProcesses
//! - WriteStdin, ReadStdout, SubscribeStdout
//!
//! ### User-Plane Operations (for multi-user isolation)
//! - Filesystem: ReadFile, WriteFile, ListDirectory, Stat, DeletePath
//! - Sessions: ListSessions, GetSession, CreateSession, StopSession
//! - Main Chat: ListMainChatSessions, GetMainChatMessages
//! - Memory: SearchMemories, AddMemory, DeleteMemory
//!
//! ### Pi Session Management
//! - PiCreateSession, PiPrompt, PiSteer, PiFollowUp, PiAbort, PiCompact
//! - PiSubscribe, PiUnsubscribe, PiListSessions, PiGetState, PiCloseSession

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::pi::PiState;

/// Request sent from octo to the runner.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunnerRequest {
    // ========================================================================
    // Process Management (original)
    // ========================================================================
    /// Spawn a detached process (fire and forget, no stdin/stdout).
    SpawnProcess(SpawnProcessRequest),

    /// Spawn a process with stdin/stdout pipes for RPC communication.
    /// Used for Pi agent which communicates via JSON-RPC over stdio.
    SpawnRpcProcess(SpawnRpcProcessRequest),

    /// Kill a process by PID.
    KillProcess(KillProcessRequest),

    /// Get status of a process.
    GetStatus(GetStatusRequest),

    /// List all managed processes.
    ListProcesses,

    /// Send data to a process's stdin (for RPC processes).
    WriteStdin(WriteStdinRequest),

    /// Read available data from a process's stdout (for RPC processes).
    ReadStdout(ReadStdoutRequest),

    /// Subscribe to stdout stream (for RPC processes).
    /// Lines are pushed as they arrive via StdoutLine responses.
    /// The subscription ends when the process exits or client disconnects.
    SubscribeStdout(SubscribeStdoutRequest),

    /// Health check.
    Ping,

    /// Shutdown the runner gracefully.
    Shutdown,

    // ========================================================================
    // Filesystem Operations (user-plane)
    // ========================================================================
    /// Read a file from the user's workspace.
    ReadFile(ReadFileRequest),

    /// Write a file to the user's workspace.
    WriteFile(WriteFileRequest),

    /// List contents of a directory.
    ListDirectory(ListDirectoryRequest),

    /// Get file/directory metadata (stat).
    Stat(StatRequest),

    /// Delete a file or directory.
    DeletePath(DeletePathRequest),

    /// Create a directory (with parents if needed).
    CreateDirectory(CreateDirectoryRequest),

    // ========================================================================
    // Session Operations (user-plane)
    // ========================================================================
    /// List all sessions for this user.
    ListSessions,

    /// Get a specific session by ID.
    GetSession(GetSessionRequest),

    /// Start services for a session.
    StartSession(StartSessionRequest),

    /// Stop a running session.
    StopSession(StopSessionRequest),

    // ========================================================================
    // Main Chat Operations (user-plane)
    // ========================================================================
    /// List main chat session files.
    ListMainChatSessions,

    /// Get messages from a main chat session.
    GetMainChatMessages(GetMainChatMessagesRequest),
    /// Get messages from a workspace Pi session (hstry-backed).
    GetWorkspaceChatMessages(GetWorkspaceChatMessagesRequest),

    // ========================================================================
    // OpenCode Chat History (user-plane)
    // ========================================================================
    /// List OpenCode chat sessions from ~/.local/share/opencode/storage/session/
    ListOpencodeSessions(ListOpencodeSessionsRequest),

    /// Get a specific OpenCode chat session.
    GetOpencodeSession(GetOpencodeSessionRequest),

    /// Get messages from an OpenCode chat session.
    GetOpencodeSessionMessages(GetOpencodeSessionMessagesRequest),

    /// Update an OpenCode chat session (e.g., rename title).
    UpdateOpencodeSession(UpdateOpencodeSessionRequest),

    // ========================================================================
    // Memory Operations (user-plane)
    // ========================================================================
    /// Search memories.
    SearchMemories(SearchMemoriesRequest),

    /// Add a new memory.
    AddMemory(AddMemoryRequest),

    /// Delete a memory by ID.
    DeleteMemory(DeleteMemoryRequest),

    // ========================================================================
    // Pi Session Management
    // ========================================================================
    /// Create or resume a Pi session.
    PiCreateSession(PiCreateSessionRequest),

    /// Close a Pi session (stop the process).
    PiCloseSession(PiCloseSessionRequest),

    /// Start a new session within existing Pi process.
    PiNewSession(PiNewSessionRequest),

    /// Switch to a different session file.
    PiSwitchSession(PiSwitchSessionRequest),

    /// List all active Pi sessions.
    PiListSessions,

    /// Subscribe to events from a Pi session.
    PiSubscribe(PiSubscribeRequest),

    /// Unsubscribe from a Pi session's events.
    PiUnsubscribe(PiUnsubscribeRequest),

    // ========================================================================
    // Pi Prompting
    // ========================================================================
    /// Send a user prompt to a Pi session.
    PiPrompt(PiPromptRequest),

    /// Send a steering message to interrupt a Pi session mid-run.
    PiSteer(PiSteerRequest),

    /// Queue a follow-up message for after the Pi session finishes.
    PiFollowUp(PiFollowUpRequest),

    /// Abort the current Pi session operation.
    PiAbort(PiAbortRequest),

    // ========================================================================
    // Pi State & Messages
    // ========================================================================
    /// Get the state of a Pi session.
    PiGetState(PiGetStateRequest),

    /// Get all messages from a Pi session.
    PiGetMessages(PiGetMessagesRequest),

    /// Get session statistics (tokens, cost, etc.).
    PiGetSessionStats(PiGetSessionStatsRequest),

    /// Get the last assistant message text.
    PiGetLastAssistantText(PiGetLastAssistantTextRequest),

    // ========================================================================
    // Pi Model Management
    // ========================================================================
    /// Set the model for a Pi session.
    PiSetModel(PiSetModelRequest),

    /// Cycle to the next available model.
    PiCycleModel(PiCycleModelRequest),

    /// Get list of available models.
    PiGetAvailableModels(PiGetAvailableModelsRequest),

    // ========================================================================
    // Pi Thinking Level
    // ========================================================================
    /// Set the thinking/reasoning level.
    PiSetThinkingLevel(PiSetThinkingLevelRequest),

    /// Cycle through thinking levels.
    PiCycleThinkingLevel(PiCycleThinkingLevelRequest),

    // ========================================================================
    // Pi Compaction
    // ========================================================================
    /// Compact the Pi session's conversation.
    PiCompact(PiCompactRequest),

    /// Enable/disable auto-compaction.
    PiSetAutoCompaction(PiSetAutoCompactionRequest),

    // ========================================================================
    // Pi Queue Modes
    // ========================================================================
    /// Set steering message delivery mode.
    PiSetSteeringMode(PiSetSteeringModeRequest),

    /// Set follow-up message delivery mode.
    PiSetFollowUpMode(PiSetFollowUpModeRequest),

    // ========================================================================
    // Pi Retry
    // ========================================================================
    /// Enable/disable auto-retry on transient errors.
    PiSetAutoRetry(PiSetAutoRetryRequest),

    /// Abort an in-progress retry.
    PiAbortRetry(PiAbortRetryRequest),

    // ========================================================================
    // Pi Forking
    // ========================================================================
    /// Fork from a previous message.
    PiFork(PiForkRequest),

    /// Get messages available for forking.
    PiGetForkMessages(PiGetForkMessagesRequest),

    // ========================================================================
    // Pi Session Metadata
    // ========================================================================
    /// Set a display name for the session.
    PiSetSessionName(PiSetSessionNameRequest),

    /// Export session to HTML.
    PiExportHtml(PiExportHtmlRequest),

    // ========================================================================
    // Pi Commands/Skills
    // ========================================================================
    /// Get available commands (extensions, templates, skills).
    PiGetCommands(PiGetCommandsRequest),

    // ========================================================================
    // Pi Bash
    // ========================================================================
    /// Execute a bash command and add output to conversation.
    PiBash(PiBashRequest),

    /// Abort a running bash command.
    PiAbortBash(PiAbortBashRequest),

    // ========================================================================
    // Pi Extension UI
    // ========================================================================
    /// Send response to an extension UI request.
    PiExtensionUiResponse(PiExtensionUiResponseRequest),
}

/// Response from runner to octo.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunnerResponse {
    // ========================================================================
    // Process Management Responses
    // ========================================================================
    /// Process spawned successfully.
    ProcessSpawned(ProcessSpawnedResponse),

    /// Process killed.
    ProcessKilled(ProcessKilledResponse),

    /// Process status.
    ProcessStatus(ProcessStatusResponse),

    /// List of managed processes.
    ProcessList(ProcessListResponse),

    /// Data written to stdin.
    StdinWritten(StdinWrittenResponse),

    /// Data read from stdout.
    StdoutRead(StdoutReadResponse),

    /// Subscription to stdout started.
    StdoutSubscribed(StdoutSubscribedResponse),

    /// A line from stdout (pushed during subscription).
    StdoutLine(StdoutLineResponse),

    /// Stdout subscription ended (process exited).
    StdoutEnd(StdoutEndResponse),

    /// Pong response to ping.
    Pong,

    /// Shutdown acknowledged.
    ShuttingDown,

    // ========================================================================
    // Filesystem Responses
    // ========================================================================
    /// File content (base64 encoded for binary safety).
    FileContent(FileContentResponse),

    /// File written successfully.
    FileWritten(FileWrittenResponse),

    /// Directory listing.
    DirectoryListing(DirectoryListingResponse),

    /// File/directory metadata.
    FileStat(FileStatResponse),

    /// Path deleted successfully.
    PathDeleted(PathDeletedResponse),

    /// Directory created successfully.
    DirectoryCreated(DirectoryCreatedResponse),

    // ========================================================================
    // Session Responses
    // ========================================================================
    /// List of sessions.
    SessionList(SessionListResponse),

    /// Single session info.
    Session(SessionResponse),

    /// Session started (with service ports/PIDs).
    SessionStarted(SessionStartedResponse),

    /// Session stopped.
    SessionStopped(SessionStoppedResponse),

    // ========================================================================
    // Main Chat Responses
    // ========================================================================
    /// List of main chat sessions.
    MainChatSessionList(MainChatSessionListResponse),

    /// Main chat messages.
    MainChatMessages(MainChatMessagesResponse),
    /// Workspace chat messages.
    WorkspaceChatMessages(MainChatMessagesResponse),

    // ========================================================================
    // OpenCode Chat History Responses
    // ========================================================================
    /// List of OpenCode chat sessions.
    OpencodeSessionList(OpencodeSessionListResponse),

    /// Single OpenCode chat session.
    OpencodeSession(OpencodeSessionResponse),

    /// OpenCode chat session messages.
    OpencodeSessionMessages(OpencodeSessionMessagesResponse),

    /// OpenCode session updated.
    OpencodeSessionUpdated(OpencodeSessionUpdatedResponse),

    // ========================================================================
    // Memory Responses
    // ========================================================================
    /// Memory search results.
    MemorySearchResults(MemorySearchResultsResponse),

    /// Memory added.
    MemoryAdded(MemoryAddedResponse),

    /// Memory deleted.
    MemoryDeleted(MemoryDeletedResponse),

    // ========================================================================
    // Pi Session Responses
    // ========================================================================
    /// Pi session created or resumed.
    PiSessionCreated(PiSessionCreatedResponse),

    /// List of Pi sessions.
    PiSessionList(PiSessionListResponse),

    /// Pi session state.
    PiState(PiStateResponse),

    /// Pi session closed.
    PiSessionClosed {
        /// The session that was closed.
        session_id: String,
    },

    /// Pi event (streamed during subscription).
    PiEvent(PiEventWrapper),

    /// Command acknowledged (for prompt, steer, follow_up, abort, compact).
    PiCommandAck {
        /// The session the command was sent to.
        session_id: String,
    },

    /// Pi subscription started.
    PiSubscribed(PiSubscribedResponse),

    /// Pi subscription ended.
    PiSubscriptionEnd(PiSubscriptionEndResponse),

    /// Pi messages response.
    PiMessages(PiMessagesResponse),

    /// Pi session stats response.
    PiSessionStats(PiSessionStatsResponse),

    /// Pi last assistant text response.
    PiLastAssistantText(PiLastAssistantTextResponse),

    /// Pi model changed response.
    PiModelChanged(PiModelChangedResponse),

    /// Pi available models response.
    PiAvailableModels(PiAvailableModelsResponse),

    /// Pi thinking level changed response.
    PiThinkingLevelChanged(PiThinkingLevelChangedResponse),

    /// Pi compaction result response.
    PiCompactionResult(PiCompactionResultResponse),

    /// Pi fork messages response.
    PiForkMessages(PiForkMessagesResponse),

    /// Pi fork result response.
    PiForkResult(PiForkResultResponse),

    /// Pi commands response.
    PiCommands(PiCommandsResponse),

    /// Pi bash result response.
    PiBashResult(PiBashResultResponse),

    /// Pi export HTML result response.
    PiExportHtmlResult(PiExportHtmlResultResponse),

    // ========================================================================
    // Generic
    // ========================================================================
    /// Generic success (for operations with no specific response data).
    Ok,

    /// Error response.
    Error(ErrorResponse),
}

// ============================================================================
// Request types
// ============================================================================

/// Request to spawn a detached process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnProcessRequest {
    /// Unique ID for this process (provided by caller for tracking).
    pub id: String,
    /// Path to the binary to execute.
    pub binary: String,
    /// Command line arguments.
    pub args: Vec<String>,
    /// Working directory (also used as sandbox workspace).
    pub cwd: PathBuf,
    /// Environment variables (merged with runner's environment).
    pub env: HashMap<String, String>,
    /// Whether to run this process in a sandbox.
    /// The runner controls sandbox configuration from its own trusted config.
    #[serde(default)]
    pub sandboxed: bool,
}

/// Request to spawn a process with RPC pipes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnRpcProcessRequest {
    /// Unique ID for this process (provided by caller for tracking).
    pub id: String,
    /// Path to the binary to execute.
    pub binary: String,
    /// Command line arguments.
    pub args: Vec<String>,
    /// Working directory (also used as sandbox workspace).
    pub cwd: PathBuf,
    /// Environment variables (merged with runner's environment).
    pub env: HashMap<String, String>,
    /// Whether to run this process in a sandbox.
    /// The runner controls sandbox configuration from its own trusted config.
    #[serde(default)]
    pub sandboxed: bool,
}

/// Request to kill a process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillProcessRequest {
    /// Process ID assigned by the runner.
    pub id: String,
    /// Force kill (SIGKILL) instead of graceful (SIGTERM).
    #[serde(default)]
    pub force: bool,
}

/// Request to get process status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetStatusRequest {
    /// Process ID assigned by the runner.
    pub id: String,
}

/// Request to write to process stdin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteStdinRequest {
    /// Process ID.
    pub id: String,
    /// Data to write (will be UTF-8 encoded).
    pub data: String,
}

/// Request to read from process stdout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadStdoutRequest {
    /// Process ID.
    pub id: String,
    /// Timeout in milliseconds (0 = non-blocking).
    #[serde(default)]
    pub timeout_ms: u64,
}

/// Request to subscribe to stdout stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeStdoutRequest {
    /// Process ID.
    pub id: String,
}

// ============================================================================
// Filesystem Request Types
// ============================================================================

/// Request to read a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadFileRequest {
    /// Path to the file (relative to workspace root or absolute within allowed roots).
    pub path: PathBuf,
    /// Optional byte offset to start reading from.
    #[serde(default)]
    pub offset: Option<u64>,
    /// Optional maximum bytes to read.
    #[serde(default)]
    pub limit: Option<u64>,
}

/// Request to write a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteFileRequest {
    /// Path to the file.
    pub path: PathBuf,
    /// File content (base64 encoded for binary safety).
    pub content_base64: String,
    /// Whether to create parent directories if they don't exist.
    #[serde(default)]
    pub create_parents: bool,
}

/// Request to list a directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListDirectoryRequest {
    /// Path to the directory.
    pub path: PathBuf,
    /// Whether to include hidden files (starting with .).
    #[serde(default)]
    pub include_hidden: bool,
}

/// Request to get file/directory metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatRequest {
    /// Path to stat.
    pub path: PathBuf,
}

/// Request to delete a path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletePathRequest {
    /// Path to delete.
    pub path: PathBuf,
    /// If true and path is a directory, delete recursively.
    #[serde(default)]
    pub recursive: bool,
}

/// Request to create a directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDirectoryRequest {
    /// Path to create.
    pub path: PathBuf,
    /// Create parent directories if they don't exist.
    #[serde(default = "default_true")]
    pub create_parents: bool,
}

fn default_true() -> bool {
    true
}

// ============================================================================
// Session Request Types
// ============================================================================

/// Request to get a specific session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetSessionRequest {
    /// Session ID.
    pub session_id: String,
}

/// Request to start session services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartSessionRequest {
    /// Session ID.
    pub session_id: String,
    /// Workspace path for the session.
    pub workspace_path: PathBuf,
    /// Port for opencode/Claude Code.
    pub opencode_port: u16,
    /// Port for fileserver.
    pub fileserver_port: u16,
    /// Port for ttyd terminal.
    pub ttyd_port: u16,
    /// Optional agent name for opencode.
    #[serde(default)]
    pub agent: Option<String>,
    /// Additional environment variables.
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Request to stop a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopSessionRequest {
    /// Session ID.
    pub session_id: String,
}

// ============================================================================
// Main Chat Request Types
// ============================================================================

/// Request to get messages from a main chat session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetMainChatMessagesRequest {
    /// Session ID (Pi session file ID).
    pub session_id: String,
    /// Optional limit on number of messages.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Request to get messages from a workspace Pi session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetWorkspaceChatMessagesRequest {
    /// Session ID (Pi session file ID).
    pub session_id: String,
    /// Workspace path to filter conversations.
    pub workspace_path: String,
    /// Optional limit on number of messages.
    #[serde(default)]
    pub limit: Option<usize>,
}

// ============================================================================
// OpenCode Chat History Request Types
// ============================================================================

/// Request to list OpenCode chat sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListOpencodeSessionsRequest {
    /// Filter by workspace path.
    #[serde(default)]
    pub workspace: Option<String>,
    /// Include child sessions (default: false).
    #[serde(default)]
    pub include_children: bool,
    /// Maximum number of sessions to return.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Request to get a specific OpenCode session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetOpencodeSessionRequest {
    /// Session ID.
    pub session_id: String,
}

/// Request to get messages from an OpenCode session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetOpencodeSessionMessagesRequest {
    /// Session ID.
    pub session_id: String,
    /// Whether to render markdown to HTML.
    #[serde(default)]
    pub render: bool,
}

/// Request to update an OpenCode session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateOpencodeSessionRequest {
    /// Session ID.
    pub session_id: String,
    /// New title (if updating).
    #[serde(default)]
    pub title: Option<String>,
}

// ============================================================================
// Memory Request Types
// ============================================================================

/// Request to search memories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMemoriesRequest {
    /// Search query.
    pub query: String,
    /// Maximum results to return.
    #[serde(default = "default_memory_limit")]
    pub limit: usize,
    /// Optional category filter.
    #[serde(default)]
    pub category: Option<String>,
}

fn default_memory_limit() -> usize {
    20
}

/// Request to add a memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMemoryRequest {
    /// Memory content.
    pub content: String,
    /// Category (e.g., "api", "architecture", "debugging").
    #[serde(default)]
    pub category: Option<String>,
    /// Importance level (1-10).
    #[serde(default)]
    pub importance: Option<u8>,
}

/// Request to delete a memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteMemoryRequest {
    /// Memory ID.
    pub memory_id: String,
}

// ============================================================================
// Pi Session Request Types
// ============================================================================

/// Request to create or resume a Pi session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiCreateSessionRequest {
    /// Unique session ID (caller-provided or generated).
    pub session_id: String,
    /// Session configuration.
    pub config: PiSessionConfig,
}

/// Configuration for a Pi session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionConfig {
    /// Working directory for Pi.
    pub cwd: PathBuf,
    /// Provider (anthropic, openai, etc.).
    #[serde(default)]
    pub provider: Option<String>,
    /// Model ID.
    #[serde(default)]
    pub model: Option<String>,
    /// Explicit session file to use (new or resume).
    #[serde(default)]
    pub session_file: Option<PathBuf>,
    /// Session file to continue from.
    #[serde(default)]
    pub continue_session: Option<PathBuf>,
    /// Environment variables for the Pi process.
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl Default for PiSessionConfig {
    fn default() -> Self {
        Self {
            cwd: PathBuf::from("."),
            provider: None,
            model: None,
            session_file: None,
            continue_session: None,
            env: HashMap::new(),
        }
    }
}

/// Request to send a prompt to a Pi session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiPromptRequest {
    /// Session ID.
    pub session_id: String,
    /// User message content.
    pub message: String,
    /// Client-generated ID for optimistic message matching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
}

/// Request to send a steering message to a Pi session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSteerRequest {
    /// Session ID.
    pub session_id: String,
    /// Steering message content.
    pub message: String,
}

/// Request to abort a Pi session's current operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiAbortRequest {
    /// Session ID.
    pub session_id: String,
}

/// Request to compact a Pi session's conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiCompactRequest {
    /// Session ID.
    pub session_id: String,
    /// Optional custom instructions for compaction.
    #[serde(default)]
    pub instructions: Option<String>,
}

/// Request to subscribe to a Pi session's events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSubscribeRequest {
    /// Session ID to subscribe to.
    pub session_id: String,
}

/// Request to unsubscribe from a Pi session's events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiUnsubscribeRequest {
    /// Session ID to unsubscribe from.
    pub session_id: String,
}

/// Request to get a Pi session's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiGetStateRequest {
    /// Session ID.
    pub session_id: String,
}

/// Request to close a Pi session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiCloseSessionRequest {
    /// Session ID.
    pub session_id: String,
}

/// Request to start a new session within existing Pi process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiNewSessionRequest {
    /// Session ID.
    pub session_id: String,
    /// Optional parent session for tracking.
    #[serde(default)]
    pub parent_session: Option<String>,
}

/// Request to switch to a different session file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSwitchSessionRequest {
    /// Session ID.
    pub session_id: String,
    /// Path to the session file to load.
    pub session_path: String,
}

/// Request to queue a follow-up message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiFollowUpRequest {
    /// Session ID.
    pub session_id: String,
    /// Follow-up message content.
    pub message: String,
}

/// Request to get all messages from a Pi session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiGetMessagesRequest {
    /// Session ID.
    pub session_id: String,
}

/// Request to get session statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiGetSessionStatsRequest {
    /// Session ID.
    pub session_id: String,
}

/// Request to get the last assistant message text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiGetLastAssistantTextRequest {
    /// Session ID.
    pub session_id: String,
}

/// Request to set the model for a Pi session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSetModelRequest {
    /// Session ID.
    pub session_id: String,
    /// Provider name.
    pub provider: String,
    /// Model ID.
    pub model_id: String,
}

/// Request to cycle to the next model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiCycleModelRequest {
    /// Session ID.
    pub session_id: String,
}

/// Request to get available models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiGetAvailableModelsRequest {
    /// Session ID.
    pub session_id: String,
}

/// Request to set the thinking level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSetThinkingLevelRequest {
    /// Session ID.
    pub session_id: String,
    /// Thinking level: off, minimal, low, medium, high, xhigh.
    pub level: String,
}

/// Request to cycle through thinking levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiCycleThinkingLevelRequest {
    /// Session ID.
    pub session_id: String,
}

/// Request to enable/disable auto-compaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSetAutoCompactionRequest {
    /// Session ID.
    pub session_id: String,
    /// Whether auto-compaction is enabled.
    pub enabled: bool,
}

/// Request to set steering message delivery mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSetSteeringModeRequest {
    /// Session ID.
    pub session_id: String,
    /// Mode: "all" or "one-at-a-time".
    pub mode: String,
}

/// Request to set follow-up message delivery mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSetFollowUpModeRequest {
    /// Session ID.
    pub session_id: String,
    /// Mode: "all" or "one-at-a-time".
    pub mode: String,
}

/// Request to enable/disable auto-retry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSetAutoRetryRequest {
    /// Session ID.
    pub session_id: String,
    /// Whether auto-retry is enabled.
    pub enabled: bool,
}

/// Request to abort an in-progress retry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiAbortRetryRequest {
    /// Session ID.
    pub session_id: String,
}

/// Request to fork from a previous message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiForkRequest {
    /// Session ID.
    pub session_id: String,
    /// Entry ID of the message to fork from.
    pub entry_id: String,
}

/// Request to get messages available for forking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiGetForkMessagesRequest {
    /// Session ID.
    pub session_id: String,
}

/// Request to set a display name for the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSetSessionNameRequest {
    /// Session ID.
    pub session_id: String,
    /// Display name for the session.
    pub name: String,
}

/// Request to export session to HTML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiExportHtmlRequest {
    /// Session ID.
    pub session_id: String,
    /// Optional output path (default: auto-generated).
    #[serde(default)]
    pub output_path: Option<String>,
}

/// Request to get available commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiGetCommandsRequest {
    /// Session ID.
    pub session_id: String,
}

/// Request to execute a bash command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiBashRequest {
    /// Session ID.
    pub session_id: String,
    /// Bash command to execute.
    pub command: String,
}

/// Request to abort a running bash command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiAbortBashRequest {
    /// Session ID.
    pub session_id: String,
}

/// Request to respond to an extension UI prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiExtensionUiResponseRequest {
    /// Session ID.
    pub session_id: String,
    /// Request ID from the extension_ui_request event.
    pub id: String,
    /// Value for select/input/editor responses.
    #[serde(default)]
    pub value: Option<String>,
    /// Confirmation for confirm responses.
    #[serde(default)]
    pub confirmed: Option<bool>,
    /// Whether the dialog was cancelled.
    #[serde(default)]
    pub cancelled: Option<bool>,
}

// ============================================================================
// Response types
// ============================================================================

/// Response when a process is spawned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSpawnedResponse {
    /// The ID provided in the request.
    pub id: String,
    /// OS process ID.
    pub pid: u32,
}

/// Response when a process is killed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessKilledResponse {
    /// The process ID.
    pub id: String,
    /// Whether the process was actually running when killed.
    pub was_running: bool,
}

/// Process status response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessStatusResponse {
    /// The process ID.
    pub id: String,
    /// Whether the process is currently running.
    pub running: bool,
    /// OS process ID (if known).
    pub pid: Option<u32>,
    /// Exit code if process has exited.
    pub exit_code: Option<i32>,
}

/// List of all managed processes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessListResponse {
    /// List of process info.
    pub processes: Vec<ProcessInfo>,
}

/// Information about a managed process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    /// Process ID assigned by runner.
    pub id: String,
    /// OS process ID.
    pub pid: u32,
    /// Binary name.
    pub binary: String,
    /// Working directory.
    pub cwd: PathBuf,
    /// Whether this is an RPC process (has stdin/stdout pipes).
    pub is_rpc: bool,
    /// Whether currently running.
    pub running: bool,
}

/// Response when stdin data is written.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdinWrittenResponse {
    /// Process ID.
    pub id: String,
    /// Number of bytes written.
    pub bytes_written: usize,
}

/// Response with stdout data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdoutReadResponse {
    /// Process ID.
    pub id: String,
    /// Data read from stdout.
    pub data: String,
    /// Whether there's more data available.
    pub has_more: bool,
}

/// Response confirming stdout subscription started.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdoutSubscribedResponse {
    /// Process ID.
    pub id: String,
}

/// A line from stdout (pushed during subscription).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdoutLineResponse {
    /// Process ID.
    pub id: String,
    /// The line content.
    pub line: String,
}

/// Stdout subscription ended.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdoutEndResponse {
    /// Process ID.
    pub id: String,
    /// Exit code if process exited.
    pub exit_code: Option<i32>,
}

// ============================================================================
// Filesystem Response Types
// ============================================================================

/// Response with file content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContentResponse {
    /// Path that was read.
    pub path: PathBuf,
    /// File content (base64 encoded).
    pub content_base64: String,
    /// Total file size in bytes.
    pub size: u64,
    /// Whether the response is truncated (more data available).
    pub truncated: bool,
}

/// Response when file is written.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWrittenResponse {
    /// Path that was written.
    pub path: PathBuf,
    /// Bytes written.
    pub bytes_written: u64,
}

/// Directory listing entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    /// Entry name (not full path).
    pub name: String,
    /// Whether this is a directory.
    pub is_dir: bool,
    /// Whether this is a symlink.
    pub is_symlink: bool,
    /// File size in bytes (0 for directories).
    pub size: u64,
    /// Last modification time (Unix timestamp ms).
    pub modified_at: i64,
}

/// Response with directory listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryListingResponse {
    /// Path that was listed.
    pub path: PathBuf,
    /// Directory entries.
    pub entries: Vec<DirEntry>,
}

/// Response with file metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStatResponse {
    /// Path that was stat'd.
    pub path: PathBuf,
    /// Whether the path exists.
    pub exists: bool,
    /// Whether this is a file.
    pub is_file: bool,
    /// Whether this is a directory.
    pub is_dir: bool,
    /// Whether this is a symlink.
    pub is_symlink: bool,
    /// File size in bytes.
    pub size: u64,
    /// Last modification time (Unix timestamp ms).
    pub modified_at: i64,
    /// Creation time (Unix timestamp ms), if available.
    pub created_at: Option<i64>,
    /// File permissions (octal, e.g., 0o644).
    pub mode: u32,
}

/// Response when path is deleted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathDeletedResponse {
    /// Path that was deleted.
    pub path: PathBuf,
}

/// Response when directory is created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryCreatedResponse {
    /// Path that was created.
    pub path: PathBuf,
}

// ============================================================================
// Session Response Types
// ============================================================================

/// Session information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Session ID.
    pub id: String,
    /// Workspace path.
    pub workspace_path: PathBuf,
    /// Session status.
    pub status: String,
    /// OpenCode/Claude Code port.
    pub opencode_port: Option<u16>,
    /// Fileserver port.
    pub fileserver_port: Option<u16>,
    /// ttyd port.
    pub ttyd_port: Option<u16>,
    /// PIDs of running processes (comma-separated).
    pub pids: Option<String>,
    /// Created at timestamp (RFC3339).
    pub created_at: String,
    /// Started at timestamp (RFC3339).
    pub started_at: Option<String>,
    /// Last activity timestamp (RFC3339).
    pub last_activity_at: Option<String>,
}

/// Response with list of sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListResponse {
    /// List of sessions.
    pub sessions: Vec<SessionInfo>,
}

/// Response with single session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResponse {
    /// Session info, or None if not found.
    pub session: Option<SessionInfo>,
}

/// Response when session is started.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStartedResponse {
    /// Session ID.
    pub session_id: String,
    /// PIDs of started processes (comma-separated).
    pub pids: String,
}

/// Response when session is stopped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStoppedResponse {
    /// Session ID.
    pub session_id: String,
}

// ============================================================================
// Main Chat Response Types
// ============================================================================

/// Main chat session info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainChatSessionInfo {
    /// Session ID.
    pub id: String,
    /// Session title (from first user message).
    pub title: Option<String>,
    /// Number of messages.
    pub message_count: usize,
    /// File size in bytes.
    pub size: u64,
    /// Last modified timestamp (Unix ms).
    pub modified_at: i64,
    /// Session start timestamp (ISO 8601).
    pub started_at: String,
}

/// Response with list of main chat sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainChatSessionListResponse {
    /// List of sessions.
    pub sessions: Vec<MainChatSessionInfo>,
}

/// Main chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainChatMessage {
    /// Message ID.
    pub id: String,
    /// Role: user, assistant, system.
    pub role: String,
    /// Message content (JSON value for structured content).
    pub content: Value,
    /// Timestamp (Unix ms).
    pub timestamp: i64,
}

/// Response with main chat messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainChatMessagesResponse {
    /// Session ID.
    pub session_id: String,
    /// Messages in chronological order.
    pub messages: Vec<MainChatMessage>,
}

// ============================================================================
// OpenCode Chat History Response Types
// ============================================================================

/// OpenCode chat session info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpencodeSessionInfo {
    /// Session ID (e.g., "ses_xxx").
    pub id: String,
    /// Human-readable ID (e.g., "cold-lamp").
    pub readable_id: String,
    /// Session title.
    pub title: Option<String>,
    /// Parent session ID (for child sessions).
    pub parent_id: Option<String>,
    /// Workspace/project path.
    pub workspace_path: String,
    /// Project name (derived from path).
    pub project_name: String,
    /// Created timestamp (ms since epoch).
    pub created_at: i64,
    /// Updated timestamp (ms since epoch).
    pub updated_at: i64,
    /// OpenCode version that created this session.
    pub version: Option<String>,
    /// Whether this session is a child session.
    pub is_child: bool,
    /// Last used model ID (from hstry conversation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Last used provider ID (from hstry conversation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

/// Response with list of OpenCode sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpencodeSessionListResponse {
    /// List of sessions.
    pub sessions: Vec<OpencodeSessionInfo>,
}

/// Response with single OpenCode session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpencodeSessionResponse {
    /// Session info, or None if not found.
    pub session: Option<OpencodeSessionInfo>,
}

/// OpenCode chat message part.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpencodeMessagePart {
    /// Part ID.
    pub id: String,
    /// Part type: "text", "tool", etc.
    pub part_type: String,
    /// Text content (for text parts).
    pub text: Option<String>,
    /// Pre-rendered HTML (if render=true was requested).
    pub text_html: Option<String>,
    /// Tool name (for tool parts).
    pub tool_name: Option<String>,
    /// Tool input (for tool parts).
    pub tool_input: Option<serde_json::Value>,
    /// Tool output (for tool parts).
    pub tool_output: Option<String>,
    /// Tool status (for tool parts).
    pub tool_status: Option<String>,
    /// Tool title/summary (for tool parts).
    pub tool_title: Option<String>,
}

/// OpenCode chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpencodeMessage {
    /// Message ID.
    pub id: String,
    /// Session ID.
    pub session_id: String,
    /// Role: user, assistant.
    pub role: String,
    /// Created timestamp (ms since epoch).
    pub created_at: i64,
    /// Completed timestamp (ms since epoch).
    pub completed_at: Option<i64>,
    /// Parent message ID.
    pub parent_id: Option<String>,
    /// Model ID.
    pub model_id: Option<String>,
    /// Provider ID.
    pub provider_id: Option<String>,
    /// Agent name.
    pub agent: Option<String>,
    /// Summary title.
    pub summary_title: Option<String>,
    /// Input tokens.
    pub tokens_input: Option<i64>,
    /// Output tokens.
    pub tokens_output: Option<i64>,
    /// Reasoning tokens.
    pub tokens_reasoning: Option<i64>,
    /// Cost in USD.
    pub cost: Option<f64>,
    /// Message parts.
    pub parts: Vec<OpencodeMessagePart>,
}

/// Response with OpenCode session messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpencodeSessionMessagesResponse {
    /// Session ID.
    pub session_id: String,
    /// Messages in chronological order.
    pub messages: Vec<OpencodeMessage>,
}

/// Response when OpenCode session is updated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpencodeSessionUpdatedResponse {
    /// Updated session info.
    pub session: OpencodeSessionInfo,
}

// ============================================================================
// Memory Response Types
// ============================================================================

/// Memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Memory ID.
    pub id: String,
    /// Memory content.
    pub content: String,
    /// Category.
    pub category: Option<String>,
    /// Importance level.
    pub importance: Option<u8>,
    /// Created at timestamp (RFC3339).
    pub created_at: String,
    /// Relevance score (for search results).
    pub score: Option<f64>,
}

/// Response with memory search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResultsResponse {
    /// Search query.
    pub query: String,
    /// Matching memories.
    pub memories: Vec<MemoryEntry>,
    /// Total matches available.
    pub total: usize,
}

/// Response when memory is added.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAddedResponse {
    /// Assigned memory ID.
    pub memory_id: String,
}

/// Response when memory is deleted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDeletedResponse {
    /// Deleted memory ID.
    pub memory_id: String,
}

// ============================================================================
// Pi Session Response Types
// ============================================================================

/// Response when a Pi session is created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionCreatedResponse {
    /// The session ID.
    pub session_id: String,
}

/// Information about a Pi session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionInfo {
    /// Session ID.
    pub session_id: String,
    /// Current session state.
    pub state: PiSessionState,
    /// Last activity timestamp (Unix ms).
    pub last_activity: i64,
    /// Number of subscribers to this session's events.
    pub subscriber_count: usize,
    /// Working directory.
    pub cwd: PathBuf,
    /// Provider (if set).
    pub provider: Option<String>,
    /// Model (if set).
    pub model: Option<String>,
}

/// Pi session lifecycle state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PiSessionState {
    /// Session is starting up.
    Starting,
    /// Session is idle, waiting for input.
    Idle,
    /// Session is streaming a response.
    Streaming,
    /// Session is compacting its context.
    Compacting,
    /// Session is aborting a running turn.
    Aborting,
    /// Session is stopping.
    Stopping,
}

/// Response with list of Pi sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionListResponse {
    /// List of sessions.
    pub sessions: Vec<PiSessionInfo>,
}

/// Response with Pi session state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiStateResponse {
    /// Session ID.
    pub session_id: String,
    /// Full Pi state from the process.
    pub state: PiState,
}

/// Canonical event wrapper for the runner IPC protocol.
///
/// Carries a canonical event from the runner to clients over Unix socket.
/// The pi_manager translates native Pi events before broadcasting.
pub type PiEventWrapper = octo_protocol::events::Event;

/// Response confirming Pi subscription started.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSubscribedResponse {
    /// Session ID.
    pub session_id: String,
}

/// Response when Pi subscription ends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSubscriptionEndResponse {
    /// Session ID.
    pub session_id: String,
    /// Reason for ending (e.g., "session_closed", "unsubscribed").
    pub reason: String,
}

/// Response with Pi session messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiMessagesResponse {
    /// Session ID.
    pub session_id: String,
    /// Messages in the session.
    pub messages: Vec<crate::pi::AgentMessage>,
}

/// Response with Pi session statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionStatsResponse {
    /// Session ID.
    pub session_id: String,
    /// Session statistics.
    pub stats: crate::pi::SessionStats,
}

/// Response with last assistant text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiLastAssistantTextResponse {
    /// Session ID.
    pub session_id: String,
    /// The text, or None if no assistant messages.
    pub text: Option<String>,
}

/// Response when model is changed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiModelChangedResponse {
    /// Session ID.
    pub session_id: String,
    /// The new model.
    pub model: crate::pi::PiModel,
    /// Current thinking level.
    pub thinking_level: String,
    /// Whether this is a scoped model.
    pub is_scoped: bool,
}

/// Response with available models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiAvailableModelsResponse {
    /// Session ID.
    pub session_id: String,
    /// Available models.
    pub models: Vec<crate::pi::PiModel>,
}

/// Response when thinking level is changed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiThinkingLevelChangedResponse {
    /// Session ID.
    pub session_id: String,
    /// The new thinking level.
    pub level: String,
}

/// Response with compaction result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiCompactionResultResponse {
    /// Session ID.
    pub session_id: String,
    /// Compaction result.
    pub result: crate::pi::CompactionResult,
}

/// Message available for forking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiForkMessage {
    /// Entry ID.
    pub entry_id: String,
    /// Message text.
    pub text: String,
}

/// Response with messages available for forking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiForkMessagesResponse {
    /// Session ID.
    pub session_id: String,
    /// Messages available for forking.
    pub messages: Vec<PiForkMessage>,
}

/// Response with fork result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiForkResultResponse {
    /// Session ID.
    pub session_id: String,
    /// The text of the message being forked from.
    pub text: String,
    /// Whether an extension cancelled the fork.
    pub cancelled: bool,
}

/// Command info (extension, template, or skill).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiCommandInfo {
    /// Command name (invoke with /name).
    pub name: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: Option<String>,
    /// Source: "extension", "template", or "skill".
    pub source: String,
    /// Location: "user", "project", or "path".
    #[serde(default)]
    pub location: Option<String>,
    /// Absolute file path to the command source.
    #[serde(default)]
    pub path: Option<String>,
}

/// Response with available commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiCommandsResponse {
    /// Session ID.
    pub session_id: String,
    /// Available commands.
    pub commands: Vec<PiCommandInfo>,
}

/// Response with bash execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiBashResultResponse {
    /// Session ID.
    pub session_id: String,
    /// Command output.
    pub output: String,
    /// Exit code.
    pub exit_code: i32,
    /// Whether the command was cancelled.
    pub cancelled: bool,
    /// Whether output was truncated.
    pub truncated: bool,
    /// Path to full output if truncated.
    #[serde(default)]
    pub full_output_path: Option<String>,
}

/// Response with HTML export result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiExportHtmlResultResponse {
    /// Session ID.
    pub session_id: String,
    /// Path to the exported HTML file.
    pub path: String,
}

// ============================================================================
// Error Types
// ============================================================================

/// Error response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error code.
    pub code: ErrorCode,
    /// Human-readable error message.
    pub message: String,
}

/// Error codes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    // Process errors
    /// Process not found.
    ProcessNotFound,
    /// Process already exists with this ID.
    ProcessAlreadyExists,
    /// Failed to spawn process.
    SpawnFailed,
    /// Failed to kill process.
    KillFailed,
    /// Process is not an RPC process (no stdin/stdout).
    NotRpcProcess,

    // Filesystem errors
    /// File or directory not found.
    PathNotFound,
    /// Path is outside allowed workspace.
    PathNotAllowed,
    /// Permission denied.
    PermissionDenied,
    /// Path already exists.
    PathExists,
    /// Not a directory.
    NotADirectory,
    /// Not a file.
    NotAFile,

    // Session errors
    /// Session not found.
    SessionNotFound,
    /// Session already exists.
    SessionExists,
    /// Session is not running.
    SessionNotRunning,
    /// Session is already running.
    SessionAlreadyRunning,

    // Pi session errors
    /// Pi session not found.
    PiSessionNotFound,
    /// Pi session already exists.
    PiSessionExists,
    /// Pi session is not in a valid state for this operation.
    PiSessionInvalidState,

    // Memory errors
    /// Memory not found.
    MemoryNotFound,

    // Sandbox errors
    /// Sandbox requested but not available or misconfigured.
    SandboxError,

    // Generic errors
    /// IO error.
    IoError,
    /// Invalid request.
    InvalidRequest,
    /// Database error.
    DatabaseError,
    /// Internal error.
    Internal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = RunnerRequest::SpawnProcess(SpawnProcessRequest {
            id: "proc-1".to_string(),
            binary: "/usr/bin/opencode".to_string(),
            args: vec![
                "serve".to_string(),
                "--port".to_string(),
                "8080".to_string(),
            ],
            cwd: PathBuf::from("/home/user/project"),
            env: HashMap::from([("FOO".to_string(), "bar".to_string())]),
            sandboxed: false,
        });

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("spawn_process"));
        assert!(json.contains("proc-1"));

        let parsed: RunnerRequest = serde_json::from_str(&json).unwrap();
        match parsed {
            RunnerRequest::SpawnProcess(p) => {
                assert_eq!(p.id, "proc-1");
                assert_eq!(p.binary, "/usr/bin/opencode");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_response_serialization() {
        let resp = RunnerResponse::ProcessSpawned(ProcessSpawnedResponse {
            id: "proc-1".to_string(),
            pid: 12345,
        });

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("process_spawned"));
        assert!(json.contains("12345"));
    }

    #[test]
    fn test_error_response() {
        let resp = RunnerResponse::Error(ErrorResponse {
            code: ErrorCode::ProcessNotFound,
            message: "No such process: foo".to_string(),
        });

        match resp {
            RunnerResponse::Error(e) => {
                assert_eq!(e.code, ErrorCode::ProcessNotFound);
                assert!(e.message.contains("foo"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_ping_pong() {
        let req = RunnerRequest::Ping;
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("ping"));

        let resp = RunnerResponse::Pong;
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("pong"));
    }

    #[test]
    fn test_pi_create_session_request() {
        let req = RunnerRequest::PiCreateSession(PiCreateSessionRequest {
            session_id: "ses_123".to_string(),
            config: PiSessionConfig {
                cwd: PathBuf::from("/home/user/project"),
                provider: Some("anthropic".to_string()),
                model: Some("claude-sonnet-4-20250514".to_string()),
                session_file: None,
                continue_session: None,
                env: HashMap::new(),
            },
        });

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("pi_create_session"));
        assert!(json.contains("ses_123"));
        assert!(json.contains("anthropic"));

        let parsed: RunnerRequest = serde_json::from_str(&json).unwrap();
        match parsed {
            RunnerRequest::PiCreateSession(p) => {
                assert_eq!(p.session_id, "ses_123");
                assert_eq!(p.config.provider.as_deref(), Some("anthropic"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_pi_event_wrapper() {
        use octo_protocol::events::{AgentPhase, EventPayload};

        let canonical_event = PiEventWrapper {
            session_id: "ses_123".to_string(),
            runner_id: "local".to_string(),
            ts: 1738764000000,
            payload: EventPayload::AgentWorking {
                phase: AgentPhase::Generating,
                detail: None,
            },
        };

        let resp = RunnerResponse::PiEvent(canonical_event);
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("pi_event"));
        assert!(json.contains("ses_123"));
        assert!(json.contains("agent.working"));
    }

    #[test]
    fn test_pi_session_state() {
        let info = PiSessionInfo {
            session_id: "ses_123".to_string(),
            state: PiSessionState::Streaming,
            last_activity: 1234567890000,
            subscriber_count: 2,
            cwd: PathBuf::from("/home/user/project"),
            provider: Some("anthropic".to_string()),
            model: Some("claude-sonnet-4-20250514".to_string()),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("streaming"));
        assert!(json.contains("subscriber_count"));
    }
}
