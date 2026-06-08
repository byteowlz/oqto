//! Compatibility projection DTOs derived from canonical timeline.
//!
//! These types are neutral protocol shapes used by storage/projection crates. Runtime
//! wire structs such as `ChatMessageProto` may convert to/from these during migration,
//! but must not be treated as the canonical durable model.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectedChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_input: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_output: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_reasoning: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    pub parts: Vec<ProjectedChatMessagePart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectedChatMessagePart {
    pub id: String,
    pub part_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_output: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectedTurnTreeNode {
    pub turn_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_turn_id: Option<String>,
    pub branch_id: String,
    pub role: String,
    pub turn_version: i64,
}

#[cfg(test)]
mod tests {
    use super::{ProjectedChatMessage, ProjectedChatMessagePart};

    #[test]
    fn projected_chat_part_preserves_structured_tool_output() {
        let part = ProjectedChatMessagePart {
            id: "part-1".to_string(),
            part_type: "tool_result".to_string(),
            text: None,
            text_html: None,
            tool_name: Some("bash".to_string()),
            tool_call_id: Some("call-1".to_string()),
            tool_input: None,
            tool_output: Some(serde_json::json!({ "stdout": "ok", "code": 0 })),
            tool_status: Some("success".to_string()),
            tool_title: None,
        };

        let json = serde_json::to_string(&part).expect("serialize projected part");
        let parsed: ProjectedChatMessagePart =
            serde_json::from_str(&json).expect("deserialize projected part");

        assert_eq!(
            parsed.tool_output,
            Some(serde_json::json!({ "stdout": "ok", "code": 0 }))
        );
    }

    #[test]
    fn projected_chat_message_accepts_legacy_string_tool_output() {
        let message: ProjectedChatMessage = serde_json::from_value(serde_json::json!({
            "id": "msg-1",
            "session_id": "session-1",
            "role": "assistant",
            "created_at": 42,
            "parts": [{
                "id": "part-1",
                "part_type": "tool_result",
                "tool_name": "bash",
                "tool_call_id": "call-1",
                "tool_output": "plain output",
                "tool_status": "success"
            }]
        }))
        .expect("deserialize projected chat message");

        assert_eq!(
            message.parts[0].tool_output,
            Some(serde_json::json!("plain output"))
        );
    }
}
