//! Direct conversions from Pi types to hstry proto types.

use hstry_core::service::proto::Message as ProtoMessage;
use serde::Serialize;
use serde_json::Value;

use crate::pi::AgentMessage;

/// Serializable message for WebSocket responses.
/// Proto messages don't implement Serialize, so we convert to this.
#[derive(Debug, Clone, Serialize)]
pub struct SerializableMessage {
    pub idx: i32,
    pub role: String,
    pub content: String,
    pub parts_json: String,
    pub created_at_ms: Option<i64>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub metadata_json: String,
    pub client_id: Option<String>,
}

impl From<&ProtoMessage> for SerializableMessage {
    fn from(msg: &ProtoMessage) -> Self {
        // model may be "provider/model" or just "model" — split if needed
        let (provider, model) = split_model_ref(&msg.model, &msg.provider);
        Self {
            idx: msg.idx,
            role: msg.role.clone(),
            content: msg.content.clone(),
            parts_json: msg.parts_json.clone(),
            created_at_ms: msg.created_at_ms,
            model,
            provider,
            tokens: msg.tokens,
            cost_usd: msg.cost_usd,
            metadata_json: msg.metadata_json.clone(),
            client_id: msg.client_id.clone(),
        }
    }
}

/// Split a combined "provider/model" string into separate (provider, model).
/// If the model field already contains a slash, split it.
/// If provider is already set separately, use that.
fn split_model_ref(
    model: &Option<String>,
    provider: &Option<String>,
) -> (Option<String>, Option<String>) {
    match (model, provider) {
        (Some(m), Some(p)) if !p.is_empty() => (Some(p.clone()), Some(m.clone())),
        (Some(m), _) if m.contains('/') => {
            let idx = m.find('/').unwrap();
            (Some(m[..idx].to_string()), Some(m[idx + 1..].to_string()))
        }
        (Some(m), _) => (None, Some(m.clone())),
        (None, _) => (None, None),
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

/// Convert a Pi AgentMessage directly to hstry proto Message.
pub fn agent_message_to_proto(msg: &AgentMessage, idx: i32) -> ProtoMessage {
    agent_message_to_proto_with_client_id(msg, idx, None)
}

/// Convert a Pi AgentMessage directly to hstry proto Message with client_id override.
pub fn agent_message_to_proto_with_client_id(
    msg: &AgentMessage,
    idx: i32,
    client_id: Option<String>,
) -> ProtoMessage {
    let role = match msg.role.as_str() {
        "user" | "human" => "user",
        "assistant" | "agent" => "assistant",
        "system" => "system",
        "tool" | "toolResult" => "tool",
        _ => "user",
    };

    let timestamp = msg
        .timestamp
        .map(|t| t as i64)
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

    let content = extract_text_content(&msg.content);
    let parts_json = build_parts_json(&msg.content, msg);

    // Build model string as "provider/model" if both present
    let model = match (&msg.provider, &msg.model) {
        (Some(provider), Some(model)) => Some(format!("{}/{}", provider, model)),
        (None, Some(model)) => Some(model.clone()),
        _ => None,
    };

    let tokens = msg.usage.as_ref().map(|u| (u.input + u.output) as i64);
    let cost_usd = msg
        .usage
        .as_ref()
        .and_then(|u| u.cost.as_ref().map(|c| c.total));

    ProtoMessage {
        idx,
        role: role.to_string(),
        content,
        parts_json,
        created_at_ms: Some(timestamp),
        model,
        tokens,
        cost_usd,
        metadata_json: String::new(),
        sender_json: String::new(),
        provider: msg.provider.clone(),
        harness: Some("pi".to_string()),
        client_id,
        id: None,
    }
}

/// Extract flattened text content from Pi message content.
fn extract_text_content(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| {
                let obj = block.as_object()?;
                if obj.get("type")?.as_str()? == "text" {
                    obj.get("text")?.as_str().map(String::from)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
        _ => String::new(),
    }
}

/// Build parts JSON from Pi message content.
fn build_parts_json(content: &Value, msg: &AgentMessage) -> String {
    let mut parts: Vec<Value> = Vec::new();
    let is_tool_result = msg.role == "tool" || msg.role == "toolResult";

    // For tool result messages, only emit the tool_result part.
    // Text content is redundant with the tool output and would leak into
    // chat as visible text if included alongside the tool_result.
    if is_tool_result {
        let tool_call_id = msg.tool_call_id.clone().unwrap_or_default();
        parts.push(serde_json::json!({
            "type": "tool_result",
            "toolCallId": tool_call_id,
            "name": msg.tool_name,
            "output": content,
            "is_error": msg.is_error.unwrap_or(false)
        }));
    } else {
        match content {
            Value::String(text) => {
                parts.push(serde_json::json!({
                    "type": "text",
                    "text": text
                }));
            }
            Value::Array(blocks) => {
                for block in blocks {
                    if let Some(obj) = block.as_object() {
                        let block_type = obj.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        match block_type {
                            "text" => {
                                if let Some(text) = obj.get("text") {
                                    parts.push(serde_json::json!({
                                        "type": "text",
                                        "text": text
                                    }));
                                }
                            }
                            "thinking" => {
                                if let Some(text) = obj.get("thinking") {
                                    parts.push(serde_json::json!({
                                        "type": "thinking",
                                        "text": text
                                    }));
                                }
                            }
                            "tool_use" => {
                                parts.push(serde_json::json!({
                                    "type": "tool_call",
                                    "id": obj.get("id").cloned().unwrap_or(Value::Null),
                                    "name": obj.get("name").cloned().unwrap_or(Value::Null),
                                    "input": obj.get("input").or(obj.get("arguments")).cloned()
                                }));
                            }
                            _ => {
                                // Pass through unknown types
                                parts.push(block.clone());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    serde_json::to_string(&parts).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_user_message() {
        let msg = AgentMessage {
            role: "user".to_string(),
            content: Value::String("Hello".to_string()),
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

        let proto = agent_message_to_proto(&msg, 0);

        assert_eq!(proto.idx, 0);
        assert_eq!(proto.role, "user");
        assert_eq!(proto.content, "Hello");
        assert_eq!(proto.created_at_ms, Some(1700000000000));
    }

    #[test]
    fn test_assistant_with_model() {
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

        let proto = agent_message_to_proto(&msg, 1);

        assert_eq!(proto.idx, 1);
        assert_eq!(proto.role, "assistant");
        assert_eq!(proto.content, "Line 1\n\nLine 2");
        assert_eq!(proto.model, Some("anthropic/claude-3-5-sonnet".to_string()));
    }
}
