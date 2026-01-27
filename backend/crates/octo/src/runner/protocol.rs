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

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

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

    // Memory errors
    /// Memory not found.
    MemoryNotFound,

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
}
