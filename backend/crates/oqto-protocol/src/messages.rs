//! Canonical message types.
//!
//! Messages are the persistent units of a conversation. They are stored in hstry
//! and rendered by the frontend. A message contains an ordered list of typed parts
//! (re-exported from `hstry_core::parts`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use hstry_core::parts::{Part, Sender};

/// A conversation message. Stored in hstry, rendered by the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique within conversation (UUID or agent-assigned).
    pub id: String,

    /// 0-based position in conversation.
    pub idx: u32,

    /// Message role.
    pub role: Role,

    /// Client-generated ID for optimistic message matching.
    /// Allows frontend to correlate provisional messages with persisted versions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// Who produced this message.
    ///
    /// Omitted for simple single-user conversations where the sender is
    /// implied by the role. Populated when:
    /// - Multiple users are in a workspace (multi-user mode)
    /// - A delegation response is inlined from another agent/session
    /// - The message comes from an external source
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender: Option<Sender>,

    /// Ordered content blocks.
    pub parts: Vec<Part>,

    /// Unix milliseconds.
    pub created_at: i64,

    // -- Assistant-specific (None for other roles) --
    /// Model ID (e.g. "claude-sonnet-4-20250514").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Provider (e.g. "anthropic").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Why generation stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,

    /// Token counts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,

    // -- Tool-result-specific (None for other roles) --
    /// Correlates to the ToolCall part this result answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,

    /// Tool name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,

    /// Whether this tool result represents an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,

    /// Agent-specific extras (forward-compatible).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

// Sender and SenderType are re-exported from hstry_core::parts via lib.rs.
// They are imported above for use in Message.

// ============================================================================
// Message metadata types
// ============================================================================

/// Message role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::Assistant => write!(f, "assistant"),
            Self::System => write!(f, "system"),
            Self::Tool => write!(f, "tool"),
        }
    }
}

/// Why generation stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Natural end of generation.
    Stop,
    /// Context window limit reached.
    Length,
    /// Agent wants to call a tool.
    ToolUse,
    /// An error occurred.
    Error,
    /// User aborted generation.
    Aborted,
}

/// Token usage for a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use hstry_core::parts::{Part, SenderType};

    #[test]
    fn test_message_serialization() {
        let msg = Message {
            id: "msg-1".to_string(),
            idx: 0,
            role: Role::Assistant,
            client_id: None,
            sender: None,
            parts: vec![Part::text("Hello, world!")],
            created_at: 1738764000000,
            model: Some("claude-sonnet-4-20250514".to_string()),
            provider: Some("anthropic".to_string()),
            stop_reason: Some(StopReason::Stop),
            usage: Some(Usage {
                input_tokens: 100,
                output_tokens: 50,
                cache_read_tokens: None,
                cache_write_tokens: None,
                cost_usd: Some(0.001),
            }),
            tool_call_id: None,
            tool_name: None,
            is_error: None,
            metadata: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"assistant\""));
        assert!(json.contains("\"stop_reason\":\"stop\""));

        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "msg-1");
        assert_eq!(parsed.role, Role::Assistant);
    }

    #[test]
    fn test_tool_message() {
        let msg = Message {
            id: "msg-2".to_string(),
            idx: 1,
            role: Role::Tool,
            client_id: None,
            sender: None,
            parts: vec![Part::tool_result(
                "call_123",
                Some(serde_json::json!("output")),
                false,
            )],
            created_at: 1738764001000,
            model: None,
            provider: None,
            stop_reason: None,
            usage: None,
            tool_call_id: Some("call_123".to_string()),
            tool_name: Some("bash".to_string()),
            is_error: Some(false),
            metadata: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"tool\""));
        assert!(json.contains("\"tool_call_id\":\"call_123\""));
    }

    #[test]
    fn test_message_with_sender() {
        let msg = Message {
            id: "msg-3".to_string(),
            idx: 2,
            role: Role::Assistant,
            client_id: None,
            sender: Some(Sender {
                sender_type: SenderType::Agent,
                id: "ses_xyz".to_string(),
                name: "pi:ses_xyz".to_string(),
                runner_id: Some("runner-alice".to_string()),
                session_id: Some("ses_xyz".to_string()),
            }),
            parts: vec![Part::text("Delegation response from another agent")],
            created_at: 1738764002000,
            model: Some("claude-sonnet-4-20250514".to_string()),
            provider: Some("anthropic".to_string()),
            stop_reason: Some(StopReason::Stop),
            usage: None,
            tool_call_id: None,
            tool_name: None,
            is_error: None,
            metadata: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"agent\""));
        assert!(json.contains("\"name\":\"pi:ses_xyz\""));
        assert!(json.contains("\"runner_id\":\"runner-alice\""));

        let parsed: Message = serde_json::from_str(&json).unwrap();
        let sender = parsed.sender.unwrap();
        assert_eq!(sender.sender_type, SenderType::Agent);
        assert_eq!(sender.name, "pi:ses_xyz");
    }

    #[test]
    fn test_sender_omitted_when_none() {
        let msg = Message {
            id: "msg-4".to_string(),
            idx: 0,
            role: Role::User,
            client_id: None,
            sender: None,
            parts: vec![Part::text("Hello")],
            created_at: 1738764000000,
            model: None,
            provider: None,
            stop_reason: None,
            usage: None,
            tool_call_id: None,
            tool_name: None,
            is_error: None,
            metadata: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("sender"));
    }
}
