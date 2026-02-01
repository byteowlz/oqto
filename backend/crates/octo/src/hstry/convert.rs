//! Conversions between Octo's Pi types and hstry proto types.

use hstry_core::service::proto::Message as ProtoMessage;
use serde::Serialize;

use crate::pi::AgentMessage;
use crate::canon::{CanonMessage, ModelInfo, pi_message_to_canon};

/// Serializable message for WebSocket responses.
#[derive(Debug, Clone, Serialize)]
pub struct SerializableMessage {
    pub idx: i32,
    pub role: String,
    pub content: String,
    pub parts_json: String,
    pub created_at_ms: Option<i64>,
    pub model: Option<String>,
    pub tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub metadata_json: String,
}

impl From<&ProtoMessage> for SerializableMessage {
    fn from(msg: &ProtoMessage) -> Self {
        Self {
            idx: msg.idx,
            role: msg.role.clone(),
            content: msg.content.clone(),
            parts_json: msg.parts_json.clone(),
            created_at_ms: msg.created_at_ms,
            model: msg.model.clone(),
            tokens: msg.tokens,
            cost_usd: msg.cost_usd,
            metadata_json: msg.metadata_json.clone(),
        }
    }
}

impl From<ProtoMessage> for SerializableMessage {
    fn from(msg: ProtoMessage) -> Self {
        Self::from(&msg)
    }
}

/// Convert proto messages to serializable form.
pub fn proto_messages_to_serializable(messages: Vec<ProtoMessage>) -> Vec<SerializableMessage> {
    messages
        .into_iter()
        .map(SerializableMessage::from)
        .collect()
}

/// Convert a canonical message to hstry proto Message.
pub fn canon_message_to_proto(msg: &CanonMessage, idx: i32) -> ProtoMessage {
    let model = msg.model.as_ref().map(ModelInfo::full_id);
    let tokens = msg.tokens.as_ref().map(|t| t.total());
    let parts_json = serde_json::to_string(&msg.parts).unwrap_or_else(|_| "[]".to_string());
    let metadata_json = msg
        .metadata
        .as_ref()
        .and_then(|m| serde_json::to_string(m).ok())
        .unwrap_or_default();

    ProtoMessage {
        idx,
        role: msg.role.to_string(),
        content: msg.content.clone(),
        parts_json,
        created_at_ms: Some(msg.created_at),
        model,
        tokens,
        cost_usd: msg.cost_usd,
        metadata_json,
    }
}

/// Convert a Pi AgentMessage to hstry proto Message using canonical conversion.
pub fn agent_message_to_proto(msg: &AgentMessage, idx: i32, session_id: &str) -> ProtoMessage {
    let canon = pi_message_to_canon(msg, session_id);
    canon_message_to_proto(&canon, idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_message_to_proto_simple() {
        let msg = AgentMessage {
            role: "user".to_string(),
            content: serde_json::Value::String("Hello".to_string()),
            timestamp: Some(1700000000000),
            tool_call_id: None,
            tool_name: None,
            is_error: None,
            api: None,
            provider: None,
            model: None,
            usage: None,
            stop_reason: None,
            extra: Default::default(),
        };

        let proto = agent_message_to_proto(&msg, 0, "ses_test");

        assert_eq!(proto.idx, 0);
        assert_eq!(proto.role, "user");
        assert_eq!(proto.content, "Hello");
        assert_eq!(proto.created_at_ms, Some(1700000000000));
    }

    #[test]
    fn test_agent_message_to_proto_with_content_blocks() {
        let msg = AgentMessage {
            role: "assistant".to_string(),
            content: serde_json::json!([
                {"type": "text", "text": "Line 1"},
                {"type": "text", "text": "Line 2"}
            ]),
            timestamp: None,
            tool_call_id: None,
            tool_name: None,
            is_error: None,
            api: None,
            provider: Some("anthropic".to_string()),
            model: Some("claude-3-5-sonnet".to_string()),
            usage: None,
            stop_reason: None,
            extra: Default::default(),
        };

        let proto = agent_message_to_proto(&msg, 1, "ses_test");

        assert_eq!(proto.idx, 1);
        assert_eq!(proto.role, "assistant");
        assert_eq!(proto.content, "Line 1\nLine 2");
        assert_eq!(proto.model, Some("anthropic/claude-3-5-sonnet".to_string()));
    }
}
