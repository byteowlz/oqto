//! Canonical command types.
//!
//! Commands flow from frontend through backend to the appropriate runner.
//! Every command gets exactly one response (delivered as an event).

use serde::{Deserialize, Serialize};

use crate::delegation::{DelegateCancelRequest, DelegateRequest};

// ============================================================================
// Command envelope
// ============================================================================

/// A canonical command with routing metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    /// Correlation ID for response matching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Target session.
    pub session_id: String,

    /// Target runner (backend resolves if omitted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_id: Option<String>,

    /// The command payload.
    #[serde(flatten)]
    pub payload: CommandPayload,
}

// ============================================================================
// Command payloads
// ============================================================================

/// All possible command types, tagged by `cmd` field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum CommandPayload {
    // -- Session lifecycle --
    /// Create a new agent session.
    #[serde(rename = "session.create")]
    SessionCreate { config: SessionConfig },

    /// Close/destroy the session.
    #[serde(rename = "session.close")]
    SessionClose,

    /// Start a new session within existing agent process.
    #[serde(rename = "session.new")]
    SessionNew {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_session: Option<String>,
    },

    /// Switch to a different session file.
    #[serde(rename = "session.switch")]
    SessionSwitch { session_path: String },

    // -- Agent commands --
    /// Send a user prompt.
    Prompt {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        images: Option<Vec<ImageAttachment>>,
        /// Client-generated ID for optimistic message matching.
        /// The frontend creates an optimistic user message with this ID,
        /// and expects it back in the persisted message so it can reconcile.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_id: Option<String>,
    },

    /// Steering message (interrupt mid-run).
    Steer { message: String },

    /// Follow-up message (queued for after current run).
    FollowUp { message: String },

    /// Abort current operation.
    Abort,

    /// Respond to an input_needed request.
    InputResponse {
        request_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confirmed: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cancelled: Option<bool>,
    },

    // -- Query commands --
    /// Get current session state.
    GetState,

    /// Get all messages.
    GetMessages,

    /// Get session statistics.
    GetStats,

    /// Get available models.
    GetModels {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workdir: Option<String>,
    },

    /// Get available commands (extensions, templates, skills).
    GetCommands,

    /// Get messages available for forking.
    GetForkPoints,

    /// List all active Pi sessions on the runner.
    /// Returns session IDs, states (idle/streaming/etc.), and working directories.
    /// Used by the frontend on reconnect to discover running sessions.
    ListSessions,

    // -- Configuration commands --
    /// Set model.
    SetModel { provider: String, model_id: String },

    /// Cycle to next model.
    CycleModel,

    /// Set thinking/reasoning level.
    SetThinkingLevel { level: String },

    /// Cycle through thinking levels.
    CycleThinkingLevel,

    /// Enable/disable auto-compaction.
    SetAutoCompaction { enabled: bool },

    /// Enable/disable auto-retry.
    SetAutoRetry { enabled: bool },

    /// Manually compact conversation.
    Compact {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instructions: Option<String>,
    },

    /// Abort an in-progress retry.
    AbortRetry,

    /// Set session display name.
    SetSessionName { name: String },

    // -- Forking --
    /// Fork from a previous message.
    Fork { entry_id: String },

    // -- Delegation --
    /// Delegate a message to another agent session.
    Delegate(DelegateRequest),

    /// Cancel an in-flight async delegation.
    #[serde(rename = "delegate.cancel")]
    DelegateCancel(DelegateCancelRequest),
}

// ============================================================================
// Supporting types
// ============================================================================

/// Configuration for creating a new agent session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Which agent harness to run ("pi", "opencode", etc.).
    pub harness: String,

    /// Working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    /// LLM provider hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Model ID hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Resume from existing session file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continue_session: Option<String>,
}

/// Image attachment for prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageAttachment {
    /// Base64-encoded image data.
    pub data: String,
    /// MIME type (e.g. "image/png").
    pub media_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_serialization() {
        let cmd = Command {
            id: Some("req-1".to_string()),
            session_id: "ses_abc".to_string(),
            runner_id: None,
            payload: CommandPayload::Prompt {
                message: "Hello".to_string(),
                images: None,
                client_id: None,
            },
        };

        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"cmd\":\"prompt\""));
        assert!(json.contains("\"message\":\"Hello\""));
        assert!(json.contains("\"session_id\":\"ses_abc\""));
    }

    #[test]
    fn test_session_create() {
        let cmd = Command {
            id: Some("req-2".to_string()),
            session_id: "ses_new".to_string(),
            runner_id: Some("local".to_string()),
            payload: CommandPayload::SessionCreate {
                config: SessionConfig {
                    harness: "pi".to_string(),
                    cwd: Some("/home/user/project".to_string()),
                    provider: Some("anthropic".to_string()),
                    model: None,
                    continue_session: None,
                },
            },
        };

        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"cmd\":\"session.create\""));
        assert!(json.contains("\"harness\":\"pi\""));

        let parsed: Command = serde_json::from_str(&json).unwrap();
        match &parsed.payload {
            CommandPayload::SessionCreate { config } => {
                assert_eq!(config.harness, "pi");
                assert_eq!(config.provider.as_deref(), Some("anthropic"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_input_response() {
        let cmd = Command {
            id: None,
            session_id: "ses_abc".to_string(),
            runner_id: None,
            payload: CommandPayload::InputResponse {
                request_id: "req-dialog-1".to_string(),
                value: Some("option_a".to_string()),
                confirmed: None,
                cancelled: None,
            },
        };

        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"cmd\":\"input_response\""));
        assert!(json.contains("\"request_id\":\"req-dialog-1\""));
    }
}
