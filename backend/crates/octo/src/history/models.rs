//! Data models for the history module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// OpenCode session as stored on disk.
/// This matches the actual structure in ~/.local/share/opencode/storage/session/
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub version: Option<String>,
    pub title: Option<String>,
    #[serde(rename = "parentID")]
    pub parent_id: Option<String>,
    /// The workspace directory path
    pub directory: Option<String>,
    /// Project ID (hash of directory)
    #[serde(rename = "projectID")]
    pub project_id: Option<String>,
    pub time: SessionTime,
}

/// Session timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTime {
    pub created: i64,
    pub updated: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct ChatSessionStats {
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub cost_usd: f64,
}

/// A chat session with its project context.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct ChatSession {
    /// Session ID (e.g., "ses_xxx")
    pub id: String,
    /// Human-readable ID (e.g., "cold-lamp") - deterministically generated from session ID
    pub readable_id: String,
    /// Session title
    pub title: Option<String>,
    /// Parent session ID (for child sessions)
    pub parent_id: Option<String>,
    /// Workspace/project path
    pub workspace_path: String,
    /// Project name (derived from path)
    pub project_name: String,
    /// Created timestamp (ms since epoch)
    pub created_at: i64,
    /// Updated timestamp (ms since epoch)
    pub updated_at: i64,
    /// OpenCode version that created this session
    pub version: Option<String>,
    /// Whether this session is a child session
    pub is_child: bool,
    /// Path to the session JSON file (for loading messages later)
    pub source_path: Option<String>,
    /// Persisted stats from hstry metadata (when available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<ChatSessionStats>,
    /// Last used model ID (from hstry conversation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Last used provider ID (from hstry conversation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HstryJsonResponse<T> {
    pub ok: bool,
    pub result: Option<T>,
    pub error: Option<String>,
}

/// A single search hit returned by hstry.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HstrySearchHit {
    pub message_id: String,
    pub conversation_id: String,
    pub message_idx: i32,
    pub role: String,
    pub content: String,
    pub snippet: String,
    pub created_at: Option<DateTime<Utc>>,
    pub conv_created_at: DateTime<Utc>,
    pub conv_updated_at: Option<DateTime<Utc>>,
    pub score: f32,
    pub source_id: String,
    pub external_id: Option<String>,
    pub title: Option<String>,
    pub workspace: Option<String>,
    pub source_adapter: String,
    pub source_path: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
}

/// Message metadata as stored in OpenCode's message directory.
#[derive(Debug, Clone, Deserialize)]
pub struct MessageInfo {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub role: String,
    pub time: MessageTime,
    #[serde(rename = "parentID")]
    pub parent_id: Option<String>,
    #[serde(rename = "modelID")]
    pub model_id: Option<String>,
    #[serde(rename = "providerID")]
    pub provider_id: Option<String>,
    pub agent: Option<String>,
    pub summary: Option<MessageSummary>,
    pub tokens: Option<TokenUsage>,
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageTime {
    pub created: i64,
    pub completed: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageSummary {
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenUsage {
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub reasoning: Option<i64>,
}

/// Message part as stored in OpenCode's part directory.
#[derive(Debug, Clone, Deserialize)]
pub struct PartInfo {
    pub id: String,
    #[serde(rename = "messageID")]
    pub message_id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "type")]
    pub part_type: String,
    /// Text content (for type="text")
    pub text: Option<String>,
    /// Tool name (for type="tool")
    pub tool: Option<String>,
    /// Tool call state (for type="tool")
    pub state: Option<ToolState>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolState {
    pub status: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<String>,
    pub title: Option<String>,
}

/// A chat message with its content parts.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub parent_id: Option<String>,
    pub model_id: Option<String>,
    pub provider_id: Option<String>,
    pub agent: Option<String>,
    pub summary_title: Option<String>,
    pub tokens_input: Option<i64>,
    pub tokens_output: Option<i64>,
    pub tokens_reasoning: Option<i64>,
    pub cost: Option<f64>,
    /// Message content parts
    pub parts: Vec<ChatMessagePart>,
}

/// A single part of a chat message.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../../frontend/src/generated/")]
pub struct ChatMessagePart {
    pub id: String,
    pub part_type: String,
    /// Text content (for text parts)
    pub text: Option<String>,
    /// Pre-rendered HTML (for text parts, when render=true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_html: Option<String>,
    /// Tool name (for tool parts)
    pub tool_name: Option<String>,
    /// Tool input (for tool parts)
    pub tool_input: Option<serde_json::Value>,
    /// Tool output (for tool parts)
    pub tool_output: Option<String>,
    /// Tool status (for tool parts)
    pub tool_status: Option<String>,
    /// Tool title/summary (for tool parts)
    pub tool_title: Option<String>,
}
