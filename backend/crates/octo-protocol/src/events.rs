//! Canonical event types.
//!
//! Events are ephemeral signals for real-time UI updates. They are NOT stored in
//! hstry as messages (but some may be logged for debugging).
//!
//! Events form a state machine: the frontend can always derive the exact UI state
//! from the current event without tracking history.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::delegation::{DelegateCompleted, DelegateDelta, DelegateError, DelegateStarted};
use crate::messages::{Message, StopReason};

// ============================================================================
// Event envelope
// ============================================================================

/// A canonical event with routing metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Which session this event belongs to.
    pub session_id: String,

    /// Which runner produced it.
    pub runner_id: String,

    /// Unix ms timestamp.
    pub ts: i64,

    /// The event payload.
    #[serde(flatten)]
    pub payload: EventPayload,
}

// ============================================================================
// Event payloads
// ============================================================================

/// All possible event types, tagged by `event` field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EventPayload {
    // -- Session lifecycle --
    /// Session created or resumed on the runner.
    #[serde(rename = "session.created")]
    SessionCreated { resumed: bool, harness: String },

    /// Session stopped/destroyed.
    #[serde(rename = "session.closed")]
    SessionClosed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },

    /// Session health heartbeat (emitted periodically by the runner).
    #[serde(rename = "session.heartbeat")]
    SessionHeartbeat { process: ProcessHealth },

    // -- Agent state --
    /// Agent is idle, waiting for input.
    #[serde(rename = "agent.idle")]
    AgentIdle,

    /// Agent is working (LLM generating, tool running, etc.).
    #[serde(rename = "agent.working")]
    AgentWorking {
        phase: AgentPhase,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },

    /// Agent encountered an error.
    #[serde(rename = "agent.error")]
    AgentError {
        error: String,
        recoverable: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        phase: Option<AgentPhase>,
    },

    /// Agent needs user input.
    #[serde(rename = "agent.input_needed")]
    AgentInputNeeded { request: InputRequest },

    /// Agent input request resolved.
    #[serde(rename = "agent.input_resolved")]
    AgentInputResolved { request_id: String },

    // -- Streaming --
    /// New message started.
    #[serde(rename = "stream.message_start")]
    StreamMessageStart { message_id: String, role: String },

    /// Text content delta.
    #[serde(rename = "stream.text_delta")]
    StreamTextDelta {
        message_id: String,
        delta: String,
        content_index: usize,
    },

    /// Thinking content delta.
    #[serde(rename = "stream.thinking_delta")]
    StreamThinkingDelta {
        message_id: String,
        delta: String,
        content_index: usize,
    },

    /// Tool call being assembled by LLM.
    #[serde(rename = "stream.tool_call_start")]
    StreamToolCallStart {
        message_id: String,
        tool_call_id: String,
        name: String,
        content_index: usize,
    },

    /// Tool call argument delta.
    #[serde(rename = "stream.tool_call_delta")]
    StreamToolCallDelta {
        message_id: String,
        tool_call_id: String,
        delta: String,
        content_index: usize,
    },

    /// Tool call complete.
    #[serde(rename = "stream.tool_call_end")]
    StreamToolCallEnd {
        message_id: String,
        tool_call_id: String,
        tool_call: ToolCallInfo,
        content_index: usize,
    },

    /// Message complete with full finalized message.
    #[serde(rename = "stream.message_end")]
    StreamMessageEnd { message: Message },

    /// Stream complete.
    #[serde(rename = "stream.done")]
    StreamDone { reason: StopReason },

    // -- Tool execution --
    /// Tool started.
    #[serde(rename = "tool.start")]
    ToolStart {
        tool_call_id: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input: Option<Value>,
    },

    /// Tool progress (accumulated output, not delta).
    #[serde(rename = "tool.progress")]
    ToolProgress {
        tool_call_id: String,
        name: String,
        partial_output: Value,
    },

    /// Tool completed.
    #[serde(rename = "tool.end")]
    ToolEnd {
        tool_call_id: String,
        name: String,
        output: Value,
        is_error: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },

    // -- Auto-recovery --
    /// Auto-retry starting.
    #[serde(rename = "retry.start")]
    RetryStart {
        attempt: u32,
        max_attempts: u32,
        delay_ms: u64,
        error: String,
    },

    /// Auto-retry result.
    #[serde(rename = "retry.end")]
    RetryEnd {
        success: bool,
        attempt: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        final_error: Option<String>,
    },

    /// Auto-compaction starting.
    #[serde(rename = "compact.start")]
    CompactStart { reason: CompactReason },

    /// Auto-compaction result.
    #[serde(rename = "compact.end")]
    CompactEnd {
        success: bool,
        will_retry: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    // -- Config changes --
    /// Model changed.
    #[serde(rename = "config.model_changed")]
    ConfigModelChanged { provider: String, model_id: String },

    /// Thinking level changed.
    #[serde(rename = "config.thinking_level_changed")]
    ConfigThinkingLevelChanged { level: String },

    // -- Notifications --
    /// Extension-originated notification.
    Notify { level: NotifyLevel, message: String },

    /// Status update (extension setStatus).
    Status {
        key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },

    // -- Messages sync --
    /// Full message list (response to get_messages or on reconnect).
    Messages { messages: Vec<Message> },

    /// Persisted count (after hstry write).
    Persisted { message_count: u64 },

    // -- Delegation --
    /// Delegation to another session started.
    #[serde(rename = "delegate.start")]
    DelegateStart(DelegateStarted),

    /// Streaming delta from delegated agent.
    #[serde(rename = "delegate.delta")]
    DelegateDelta(DelegateDelta),

    /// Delegation completed successfully.
    #[serde(rename = "delegate.end")]
    DelegateEnd(DelegateCompleted),

    /// Delegation failed.
    #[serde(rename = "delegate.error")]
    DelegateError(DelegateError),

    // -- Command response --
    /// Response to a command.
    Response(CommandResponse),
}

// ============================================================================
// Supporting types
// ============================================================================

/// What the agent is currently doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentPhase {
    /// LLM is producing tokens.
    Generating,
    /// LLM is in extended thinking mode.
    Thinking,
    /// Tool is executing.
    ToolRunning,
    /// Context compaction in progress.
    Compacting,
    /// Auto-retry after transient error.
    Retrying,
    /// Session starting up.
    Initializing,
}

impl std::fmt::Display for AgentPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Generating => write!(f, "generating"),
            Self::Thinking => write!(f, "thinking"),
            Self::ToolRunning => write!(f, "tool_running"),
            Self::Compacting => write!(f, "compacting"),
            Self::Retrying => write!(f, "retrying"),
            Self::Initializing => write!(f, "initializing"),
        }
    }
}

impl AgentPhase {
    /// Parse from the octo-bridge extension's status value.
    ///
    /// The extension emits values like "generating", "tool_running:bash", "compacting".
    /// This parses the phase portion (before the colon).
    pub fn from_extension_status(status: &str) -> Option<(Self, Option<String>)> {
        let (phase_str, detail) = match status.split_once(':') {
            Some((p, d)) => (p, Some(d.to_string())),
            None => (status, None),
        };

        let phase = match phase_str {
            "generating" => Self::Generating,
            "thinking" => Self::Thinking,
            "tool_running" => Self::ToolRunning,
            "compacting" => Self::Compacting,
            "retrying" => Self::Retrying,
            "initializing" => Self::Initializing,
            _ => return None,
        };

        Some((phase, detail))
    }
}

/// Process health information from the runner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessHealth {
    pub alive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rss_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime_s: Option<u64>,
}

/// Agent input request types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputRequest {
    Select {
        request_id: String,
        title: String,
        options: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
    },
    Confirm {
        request_id: String,
        title: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
    },
    Input {
        request_id: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
    },
    Permission {
        request_id: String,
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<Value>,
    },
}

/// Completed tool call info (included in stream.tool_call_end).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallInfo {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// Reason for compaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactReason {
    Threshold,
    Overflow,
}

/// Notification severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotifyLevel {
    Info,
    Warning,
    Error,
}

/// Response to a command (delivered as an event).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    /// Echoed correlation ID from the command.
    pub id: String,
    /// Which command this responds to.
    pub cmd: String,
    /// Whether the command succeeded.
    pub success: bool,
    /// Command-specific response data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    /// Error message on failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_phase_from_extension_status() {
        let (phase, detail) = AgentPhase::from_extension_status("generating").unwrap();
        assert_eq!(phase, AgentPhase::Generating);
        assert!(detail.is_none());

        let (phase, detail) = AgentPhase::from_extension_status("tool_running:bash").unwrap();
        assert_eq!(phase, AgentPhase::ToolRunning);
        assert_eq!(detail.as_deref(), Some("bash"));

        assert!(AgentPhase::from_extension_status("unknown_phase").is_none());
    }

    #[test]
    fn test_event_serialization() {
        let event = Event {
            session_id: "ses_abc".to_string(),
            runner_id: "local".to_string(),
            ts: 1738764000000,
            payload: EventPayload::AgentWorking {
                phase: AgentPhase::Generating,
                detail: None,
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"agent.working\""));
        assert!(json.contains("\"phase\":\"generating\""));
        assert!(json.contains("\"session_id\":\"ses_abc\""));
    }

    #[test]
    fn test_stream_text_delta() {
        let event = Event {
            session_id: "ses_abc".to_string(),
            runner_id: "local".to_string(),
            ts: 1738764000000,
            payload: EventPayload::StreamTextDelta {
                message_id: "msg-1".to_string(),
                delta: "Hello".to_string(),
                content_index: 0,
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"stream.text_delta\""));
        assert!(json.contains("\"delta\":\"Hello\""));
    }

    #[test]
    fn test_command_response_event_serialization() {
        // Verify that CommandResponse fields are flattened into the event
        // (not nested under a "response" key). This is the wire format the
        // frontend relies on.
        let event = Event {
            session_id: "ses_abc".to_string(),
            runner_id: "local".to_string(),
            ts: 1738764000000,
            payload: EventPayload::Response(CommandResponse {
                id: "req-1".to_string(),
                cmd: "session.create".to_string(),
                success: true,
                data: Some(serde_json::json!({"session_id": "ses_abc"})),
                error: None,
            }),
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["event"], "response");
        assert_eq!(parsed["session_id"], "ses_abc");
        // CommandResponse fields must be at top level, not nested
        assert_eq!(parsed["id"], "req-1");
        assert_eq!(parsed["cmd"], "session.create");
        assert_eq!(parsed["success"], true);
        assert!(parsed.get("data").is_some());
        // Must NOT have a "response" wrapper object
        assert!(parsed.get("response").is_none());
    }

    #[test]
    fn test_input_request_serialization() {
        let req = InputRequest::Select {
            request_id: "req-1".to_string(),
            title: "Choose model".to_string(),
            options: vec!["gpt-4".to_string(), "claude-3".to_string()],
            timeout: Some(30000),
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"type\":\"select\""));
        assert!(json.contains("Choose model"));
    }
}
