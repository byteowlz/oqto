//! Main Chat data models.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt;

/// Type of history entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryEntryType {
    /// Summary of a session or compaction
    Summary,
    /// Important decision made
    Decision,
    /// State handoff for next session
    Handoff,
    /// Insight to be stored in mmry
    Insight,
}

impl fmt::Display for HistoryEntryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Summary => write!(f, "summary"),
            Self::Decision => write!(f, "decision"),
            Self::Handoff => write!(f, "handoff"),
            Self::Insight => write!(f, "insight"),
        }
    }
}

impl std::str::FromStr for HistoryEntryType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "summary" => Ok(Self::Summary),
            "decision" => Ok(Self::Decision),
            "handoff" => Ok(Self::Handoff),
            "insight" => Ok(Self::Insight),
            _ => Err(format!("Unknown history entry type: {}", s)),
        }
    }
}

/// A history entry representing a summary, decision, or handoff.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct HistoryEntry {
    /// Auto-incrementing ID
    pub id: i64,
    /// ISO timestamp
    pub ts: String,
    /// Entry type
    #[sqlx(rename = "type")]
    pub entry_type: String,
    /// The actual content
    pub content: String,
    /// OpenCode session ID this came from
    pub session_id: Option<String>,
    /// JSON metadata blob
    pub meta: Option<String>,
    /// When this was created
    pub created_at: String,
}

/// Input for creating a new history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateHistoryEntry {
    /// Entry type
    #[serde(rename = "type")]
    pub entry_type: HistoryEntryType,
    /// The actual content
    pub content: String,
    /// OpenCode session ID this came from
    pub session_id: Option<String>,
    /// Optional metadata
    pub meta: Option<serde_json::Value>,
}

/// A Main Chat session linking an OpenCode session to this assistant.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MainChatSession {
    /// Auto-incrementing ID
    pub id: i64,
    /// OpenCode session ID
    pub session_id: String,
    /// Session title
    pub title: Option<String>,
    /// When the session started
    pub started_at: String,
    /// When the session ended (if ended)
    pub ended_at: Option<String>,
    /// Number of messages in this session
    pub message_count: i64,
}

/// Input for creating a new session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSession {
    /// OpenCode session ID
    pub session_id: String,
    /// Optional title
    pub title: Option<String>,
}

/// Full assistant info including metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantInfo {
    /// Assistant name
    pub name: String,
    /// User ID
    pub user_id: String,
    /// Path to assistant directory
    pub path: String,
    /// Number of sessions
    pub session_count: i64,
    /// Number of history entries
    pub history_count: i64,
    /// When the assistant was created
    pub created_at: Option<String>,
}

// ========== Chat Message Types ==========

/// Role of a chat message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl fmt::Display for MessageRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::Assistant => write!(f, "assistant"),
            Self::System => write!(f, "system"),
        }
    }
}

impl std::str::FromStr for MessageRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "system" => Ok(Self::System),
            _ => Err(format!("Unknown message role: {}", s)),
        }
    }
}

/// A chat message stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChatMessage {
    /// Auto-incrementing ID
    pub id: i64,
    /// Message role (user, assistant, system)
    pub role: String,
    /// Message content (JSON array of parts)
    pub content: String,
    /// Pi session ID this message belongs to
    pub pi_session_id: Option<String>,
    /// Unix timestamp in milliseconds
    pub timestamp: i64,
    /// When this was created
    pub created_at: String,
}

/// Input for creating a new chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChatMessage {
    /// Message role
    pub role: MessageRole,
    /// Message content (JSON array of parts)
    pub content: serde_json::Value,
    /// Pi session ID
    pub pi_session_id: Option<String>,
}
