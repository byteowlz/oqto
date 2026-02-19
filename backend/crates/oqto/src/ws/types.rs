//! WebSocket message types for unified real-time communication.
//!
//! These types define the protocol between frontend and backend over WebSocket.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ============================================================================
// Events (Server -> Client)
// ============================================================================

/// Spotlight step definition for UI tours.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSpotlightStep {
    pub target: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub position: Option<String>,
}

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

    /// Session error (legacy).
    SessionError {
        session_id: String,
        error_type: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<Value>,
    },

    // ========== Agent Connection Events ==========
    /// Agent connected and ready.
    AgentConnected { session_id: String },

    /// Agent disconnected.
    AgentDisconnected { session_id: String, reason: String },

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
    MessageUpdated { session_id: String, message: Value },

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
    /// Matches permission type structure.
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

    // ========== Question Events ==========
    /// Question request from agent (user question / multiple choice).
    /// Matches question.Request type structure.
    QuestionRequest {
        session_id: String,
        request_id: String,
        /// Array of questions to ask
        questions: Value,
        /// Optional tool context
        #[serde(skip_serializing_if = "Option::is_none")]
        tool: Option<Value>,
    },

    /// Question request resolved.
    QuestionResolved {
        session_id: String,
        request_id: String,
    },

    // ========== A2UI Events ==========
    /// A2UI surface from agent.
    /// Contains A2UI messages for rendering interactive UI.
    A2uiSurface {
        session_id: String,
        surface_id: String,
        /// A2UI messages (surfaceUpdate, dataModelUpdate, beginRendering, deleteSurface)
        messages: Value,
        /// Whether agent is waiting for user response
        #[serde(default)]
        blocking: bool,
        /// Request ID for blocking surfaces
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },

    /// A2UI user action response.
    A2uiActionResolved {
        session_id: String,
        request_id: String,
    },

    // ========== UI Control Events ==========
    /// Navigate to a route/path.
    #[serde(rename = "ui.navigate")]
    UiNavigate { path: String, replace: bool },

    /// Switch active session.
    #[serde(rename = "ui.session")]
    UiSession {
        session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
    },

    /// Switch active view inside the session UI.
    #[serde(rename = "ui.view")]
    UiView { view: String },

    /// Open or close the command palette.
    #[serde(rename = "ui.palette")]
    UiPalette { open: bool },

    /// Execute a command palette action directly.
    #[serde(rename = "ui.palette_exec")]
    UiPaletteExec {
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        args: Option<Value>,
    },

    /// Spotlight a specific UI element.
    #[serde(rename = "ui.spotlight")]
    UiSpotlight {
        #[serde(skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        action: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<String>,
        active: bool,
    },

    /// Tour mode for sequential spotlights.
    #[serde(rename = "ui.tour")]
    UiTour {
        steps: Vec<UiSpotlightStep>,
        #[serde(skip_serializing_if = "Option::is_none")]
        start_index: Option<u32>,
        active: bool,
    },

    /// Collapse/expand the left sidebar.
    #[serde(rename = "ui.sidebar")]
    UiSidebar {
        #[serde(skip_serializing_if = "Option::is_none")]
        collapsed: Option<bool>,
    },

    /// Control right panel or expanded panel state.
    #[serde(rename = "ui.panel")]
    UiPanel {
        #[serde(skip_serializing_if = "Option::is_none")]
        view: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        collapsed: Option<bool>,
    },

    /// Switch theme.
    #[serde(rename = "ui.theme")]
    UiTheme { theme: String },

    // ========== Legacy Events ==========
    /// Legacy SSE event (deprecated).
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

    // ========== Question Commands ==========
    /// Reply to a question request.
    QuestionReply {
        session_id: String,
        request_id: String,
        /// Array of answers (each answer is an array of selected labels)
        answers: Value,
    },

    /// Reject/dismiss a question request.
    QuestionReject {
        session_id: String,
        request_id: String,
    },

    // ========== A2UI Commands ==========
    /// Send A2UI user action response.
    A2uiAction {
        session_id: String,
        surface_id: String,
        /// Request ID for blocking surfaces
        #[serde(default)]
        request_id: Option<String>,
        /// Action name from the component
        action_name: String,
        /// Source component ID
        source_component_id: String,
        /// Resolved context from action
        context: Value,
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

impl WsCommand {
    pub fn session_id(&self) -> Option<&str> {
        match self {
            WsCommand::Subscribe { session_id }
            | WsCommand::Unsubscribe { session_id }
            | WsCommand::SendMessage { session_id, .. }
            | WsCommand::SendParts { session_id, .. }
            | WsCommand::Abort { session_id }
            | WsCommand::PermissionReply { session_id, .. }
            | WsCommand::QuestionReply { session_id, .. }
            | WsCommand::QuestionReject { session_id, .. }
            | WsCommand::A2uiAction { session_id, .. }
            | WsCommand::RefreshSession { session_id }
            | WsCommand::GetMessages { session_id, .. } => Some(session_id),
            WsCommand::Pong => None,
        }
    }
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

#[cfg(test)]
mod tests_a2ui {
    use super::*;

    #[test]
    fn test_a2ui_surface_serialization() {
        let event = WsEvent::A2uiSurface {
            session_id: "test-session".to_string(),
            surface_id: "surface-1".to_string(),
            messages: serde_json::json!([{"test": "msg"}]),
            blocking: true,
            request_id: Some("req-123".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();
        println!("A2UI Surface JSON: {}", json);
        assert!(json.contains("\"type\":\"a2ui_surface\""));
        assert!(json.contains("\"session_id\":\"test-session\""));
        assert!(json.contains("\"surface_id\":\"surface-1\""));
        assert!(json.contains("\"blocking\":true"));
    }
}
