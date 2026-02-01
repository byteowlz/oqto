//! Conversions between Octo's Pi types and hstry proto types.

use hstry_core::service::proto::Message as ProtoMessage;

use crate::pi::AgentMessage;

/// Convert a Pi AgentMessage to hstry proto Message.
///
/// This is used when persisting the full conversation from Pi's AgentEnd event
/// to the hstry daemon.
pub fn agent_message_to_proto(msg: &AgentMessage, idx: i32) -> ProtoMessage {
    // Flatten content to string
    let content = match &msg.content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => {
            // Content blocks array - extract text parts
            arr.iter()
                .filter_map(|block| {
                    if let Some(obj) = block.as_object() {
                        if obj.get("type").and_then(|t| t.as_str()) == Some("text") {
                            return obj.get("text").and_then(|t| t.as_str()).map(String::from);
                        }
                    }
                    None
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        other => other.to_string(),
    };

    // Store the original content structure in parts_json
    let parts_json = serde_json::to_string(&msg.content).unwrap_or_else(|_| "[]".to_string());

    // Extract tokens from usage
    let tokens = msg.usage.as_ref().map(|u| (u.input + u.output) as i64);

    // Extract cost from usage
    let cost_usd = msg
        .usage
        .as_ref()
        .and_then(|u| u.cost.as_ref())
        .map(|c| c.total);

    // Build model string
    let model = match (&msg.provider, &msg.model) {
        (Some(provider), Some(model)) => Some(format!("{}/{}", provider, model)),
        (None, Some(model)) => Some(model.clone()),
        _ => None,
    };

    // Convert timestamp (milliseconds since epoch)
    let created_at_ms = msg.timestamp.map(|t| t as i64);

    ProtoMessage {
        idx,
        role: msg.role.clone(),
        content,
        parts_json,
        created_at_ms,
        model,
        tokens,
        cost_usd,
        metadata_json: String::new(),
    }
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
            api: None,
            provider: None,
            model: None,
            usage: None,
            stop_reason: None,
        };

        let proto = agent_message_to_proto(&msg, 0);

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
            api: None,
            provider: Some("anthropic".to_string()),
            model: Some("claude-3-5-sonnet".to_string()),
            usage: None,
            stop_reason: None,
        };

        let proto = agent_message_to_proto(&msg, 1);

        assert_eq!(proto.idx, 1);
        assert_eq!(proto.role, "assistant");
        assert_eq!(proto.content, "Line 1\nLine 2");
        assert_eq!(proto.model, Some("anthropic/claude-3-5-sonnet".to_string()));
    }
}
