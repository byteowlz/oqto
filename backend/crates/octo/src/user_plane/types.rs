//! Types for user-plane operations.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// File content response.
#[derive(Debug, Clone)]
pub struct FileContent {
    /// File content as bytes.
    pub content: Vec<u8>,
    /// Total file size in bytes.
    pub size: u64,
    /// Whether the response is truncated.
    pub truncated: bool,
}

/// Directory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    /// Entry name (not full path).
    pub name: String,
    /// Whether this is a directory.
    pub is_dir: bool,
    /// Whether this is a symlink.
    pub is_symlink: bool,
    /// File size in bytes.
    pub size: u64,
    /// Last modification time (Unix timestamp ms).
    pub modified_at: i64,
}

/// File/directory metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStat {
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
    /// File permissions (octal).
    pub mode: u32,
}

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
    /// PIDs of running processes.
    pub pids: Option<String>,
    /// Created at timestamp (RFC3339).
    pub created_at: String,
    /// Started at timestamp (RFC3339).
    pub started_at: Option<String>,
    /// Last activity timestamp (RFC3339).
    pub last_activity_at: Option<String>,
}

/// Request to start a session.
#[derive(Debug, Clone)]
pub struct StartSessionRequest {
    /// Session ID.
    pub session_id: String,
    /// Workspace path.
    pub workspace_path: PathBuf,
    /// Port for opencode/Claude Code.
    pub opencode_port: u16,
    /// Port for fileserver.
    pub fileserver_port: u16,
    /// Port for ttyd terminal.
    pub ttyd_port: u16,
    /// Optional agent name.
    pub agent: Option<String>,
    /// Additional environment variables.
    pub env: HashMap<String, String>,
}

/// Response from starting a session.
#[derive(Debug, Clone)]
pub struct StartSessionResponse {
    /// Session ID.
    pub session_id: String,
    /// PIDs of started processes.
    pub pids: String,
}

/// Main chat session info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainChatSessionInfo {
    /// Session ID.
    pub id: String,
    /// Session title.
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

/// Main chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainChatMessage {
    /// Message ID.
    pub id: String,
    /// Role: user, assistant, system.
    pub role: String,
    /// Message content.
    pub content: Value,
    /// Timestamp (Unix ms).
    pub timestamp: i64,
}

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
    /// Relevance score.
    pub score: Option<f64>,
}

/// Memory search results.
#[derive(Debug, Clone)]
pub struct MemorySearchResults {
    /// Matching memories.
    pub memories: Vec<MemoryEntry>,
    /// Total matches available.
    pub total: usize,
}
