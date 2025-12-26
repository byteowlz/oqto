//! Common types for the AgentRPC interface.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A conversation (chat session) from opencode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    /// Unique conversation ID (e.g., "ses_xxx")
    pub id: String,
    /// Human-readable title
    pub title: Option<String>,
    /// Parent conversation ID (for child/branched sessions)
    pub parent_id: Option<String>,
    /// Working directory for this conversation
    pub workspace_path: String,
    /// Project name (derived from workspace path)
    pub project_name: String,
    /// Creation timestamp (milliseconds since epoch)
    pub created_at: i64,
    /// Last update timestamp (milliseconds since epoch)
    pub updated_at: i64,
    /// Whether this is currently active/running
    pub is_active: bool,
    /// OpenCode version
    pub version: Option<String>,
}

/// A message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message ID
    pub id: String,
    /// Conversation/session ID
    pub session_id: String,
    /// Role: "user" or "assistant"
    pub role: String,
    /// Message parts (text, tool calls, etc.)
    pub parts: Vec<MessagePart>,
    /// Creation timestamp
    pub created_at: i64,
    /// Completion timestamp (for assistant messages)
    pub completed_at: Option<i64>,
    /// Model used (for assistant messages)
    pub model: Option<MessageModel>,
    /// Token usage
    pub tokens: Option<TokenUsage>,
}

/// A part of a message (text, tool call, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessagePart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool")]
    Tool {
        tool: String,
        #[serde(rename = "callID")]
        call_id: Option<String>,
        state: Option<ToolState>,
    },
    #[serde(rename = "step-start")]
    StepStart,
    #[serde(rename = "step-finish")]
    StepFinish {
        reason: Option<String>,
        cost: Option<f64>,
        tokens: Option<TokenUsage>,
    },
    #[serde(other)]
    Unknown,
}

/// Tool execution state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolState {
    pub status: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<String>,
    pub title: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Model information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageModel {
    #[serde(rename = "providerID")]
    pub provider_id: String,
    #[serde(rename = "modelID")]
    pub model_id: String,
}

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub reasoning: Option<i64>,
    pub cache: Option<TokenCache>,
}

/// Token cache statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCache {
    pub read: Option<i64>,
    pub write: Option<i64>,
}

/// Options for starting a session.
#[derive(Debug, Clone, Default)]
pub struct StartSessionOpts {
    /// Model to use (provider/model format)
    pub model: Option<String>,
    /// Agent to use (passed to opencode via --agent flag)
    pub agent: Option<String>,
    /// Session ID to resume (if any)
    pub resume_session_id: Option<String>,
    /// Project ID for shared project sessions.
    /// When set, the session runs as the project's Linux user instead of
    /// the requesting user's Linux user, enabling multi-user access.
    pub project_id: Option<String>,
    /// Additional environment variables
    pub env: HashMap<String, String>,
}

/// Handle to a running session.
#[derive(Debug, Clone, Serialize)]
pub struct SessionHandle {
    /// Platform session ID (from octo database)
    pub session_id: String,
    /// OpenCode session ID (may differ from platform ID)
    pub opencode_session_id: Option<String>,
    /// Base URL for the opencode API
    pub api_url: String,
    /// Port for the opencode API
    pub opencode_port: u16,
    /// Port for ttyd terminal
    pub ttyd_port: u16,
    /// Port for fileserver
    pub fileserver_port: u16,
    /// Working directory
    pub workdir: String,
    /// Whether this is a newly created session or resumed
    pub is_new: bool,
}

/// Request to send a message.
#[derive(Debug, Clone)]
pub struct SendMessageRequest {
    /// Message content parts
    pub parts: Vec<SendMessagePart>,
    /// Model override
    pub model: Option<MessageModel>,
}

/// Part of a message to send.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum SendMessagePart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "file")]
    File {
        mime: String,
        url: String,
        filename: Option<String>,
    },
    #[serde(rename = "agent")]
    Agent { name: String, id: Option<String> },
}

/// Health status of the backend.
#[derive(Debug, Clone, Serialize)]
pub struct HealthStatus {
    /// Whether the backend is healthy
    pub healthy: bool,
    /// Backend mode (local/container)
    pub mode: String,
    /// Version info
    pub version: Option<String>,
    /// Additional details
    pub details: Option<String>,
}
