//! WebSocket message types for unified real-time communication.
//!
//! These types define the protocol between frontend and backend over WebSocket.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ============================================================================
// Events (Server -> Client)
// ============================================================================

/// Events sent from backend to frontend over WebSocket.
///
/// All events are tagged with a session_id to allow multiplexing multiple
/// sessions over a single WebSocket connection.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    // ========== Connection Events ==========
    /// WebSocket connection established.
    Connected,

    /// Heartbeat/keepalive ping.
    Ping,

    /// Error message.
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
    },

    // ========== Session Lifecycle Events ==========
    /// Session created or updated.
    SessionUpdated {
        session_id: String,
        status: String,
        workspace_path: String,
    },

    /// Session error from OpenCode.
    SessionError {
        session_id: String,
        error_type: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
    },

    // ========== Agent Connection Events ==========
    /// Agent (OpenCode/Pi) connected and ready.
    AgentConnected { session_id: String },

    /// Agent disconnected.
    AgentDisconnected {
        session_id: String,
        reason: String,
    },

    /// Attempting to reconnect to agent.
    AgentReconnecting {
        session_id: String,
        attempt: u32,
        delay_ms: u64,
    },

    // ========== Agent Runtime Events ==========
    /// Session is busy (agent working).
    SessionBusy { session_id: String },

    /// Session is idle (agent ready).
    SessionIdle { session_id: String },

    // ========== Message Streaming Events ==========
    /// Text content delta (streaming).
    TextDelta {
        session_id: String,
        message_id: String,
        delta: String,
    },

    /// Thinking/reasoning content delta (streaming).
    ThinkingDelta {
        session_id: String,
        message_id: String,
        delta: String,
    },

    /// Full message update (for non-streaming updates).
    MessageUpdated {
        session_id: String,
        message: Value,
    },

    // ========== Tool Events ==========
    /// Tool execution started.
    ToolStart {
        session_id: String,
        tool_call_id: String,
        tool_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        input: Option<Value>,
    },

    /// Tool execution completed.
    ToolEnd {
        session_id: String,
        tool_call_id: String,
        tool_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
        is_error: bool,
    },

    // ========== Permission Events ==========
    /// Permission request from agent.
    /// Matches OpenCode SDK Permission type structure.
    PermissionRequest {
        session_id: String,
        permission_id: String,
        /// Permission type (e.g., "bash", "edit", "webfetch")
        permission_type: String,
        /// Human-readable title/description
        title: String,
        /// Optional pattern (e.g., command for bash, file path for edit)
        #[serde(skip_serializing_if = "Option::is_none")]
        pattern: Option<Value>,
        /// Additional metadata
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<Value>,
    },

    /// Permission request resolved.
    PermissionResolved {
        session_id: String,
        permission_id: String,
        granted: bool,
    },

    // ========== Compaction Events ==========
    // ========== OpenCode-Specific Events ==========
    /// Raw OpenCode SSE event (for backwards compatibility).
    /// Contains the original event type and data.
    OpencodeEvent {
        session_id: String,
        event_type: String,
        data: Value,
    },
}

// ============================================================================
// Commands (Client -> Server)
// ============================================================================

/// Commands sent from frontend to backend over WebSocket.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsCommand {
    // ========== Connection Commands ==========
    /// Pong response to ping.
    Pong,

    // ========== Session Commands ==========
    /// Subscribe to events for a session.
    Subscribe { session_id: String },

    /// Unsubscribe from a session.
    Unsubscribe { session_id: String },

    // ========== Agent Commands ==========
    /// Send a message to the agent.
    SendMessage {
        session_id: String,
        message: String,
        #[serde(default)]
        attachments: Vec<Attachment>,
    },

    /// Send message parts (for multi-part messages).
    SendParts {
        session_id: String,
        parts: Vec<MessagePart>,
    },

    /// Abort current agent operation.
    Abort { session_id: String },

    // ========== Permission Commands ==========
    /// Reply to a permission request.
    PermissionReply {
        session_id: String,
        permission_id: String,
        granted: bool,
    },

    // ========== Session Management ==========
    /// Request session state refresh.
    RefreshSession { session_id: String },

    /// Request messages for a session.
    GetMessages {
        session_id: String,
        #[serde(default)]
        after_id: Option<String>,
    },
}

/// Attachment for messages.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Attachment {
    #[serde(rename = "type")]
    pub attachment_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

/// Message part for multi-part messages.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessagePart {
    Text { text: String },
    Image { url: String },
    File { path: String },
}

// ============================================================================
// Internal Types
// ============================================================================

/// Subscription info for a user's session.
#[derive(Debug, Clone)]
pub struct SessionSubscription {
    pub session_id: String,
    pub workspace_path: String,
    pub opencode_port: u16,
}

/// Connection state for an agent adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Failed,
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionState::Disconnected => write!(f, "disconnected"),
            ConnectionState::Connecting => write!(f, "connecting"),
            ConnectionState::Connected => write!(f, "connected"),
            ConnectionState::Reconnecting => write!(f, "reconnecting"),
            ConnectionState::Failed => write!(f, "failed"),
        }
    }
}
