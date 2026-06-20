//! Deterministic native event/message to canonical timeline projectors.
//!
//! These projectors are pure: the same native source always yields the same
//! timeline graph. Storage/import code can persist the resulting document or use
//! it as a golden source for validation.

use anyhow::Result;
use oqto_pi::AgentMessage;
use oqto_protocol::messages::Role;
use oqto_protocol::timeline::{
    RawEnvelope, TimelineBranch, TimelineDocument, TimelineMessage, TimelinePart, TimelineSession,
    TimelineSource, TimelineTurn, ToolLifecycleStatus, TurnStatus,
};
use serde_json::Value;

use crate::oqto_log::ids::{MessageIdInput, TurnIdInput, derive_message_id, derive_turn_id};
use crate::oqto_log::projection::validate_timeline;

#[derive(Debug, Clone)]
pub struct PiTimelineProjectionInput<'a> {
    pub session_id: &'a str,
    pub platform_id: &'a str,
    pub external_id: Option<&'a str>,
    pub user_id: &'a str,
    pub workspace_id: Option<&'a str>,
    pub source_session_id: &'a str,
    pub source_kind: &'a str,
    pub messages: &'a [AgentMessage],
}

pub fn project_pi_messages_to_timeline(
    input: &PiTimelineProjectionInput<'_>,
) -> Result<TimelineDocument> {
    let branch_id = format!("branch:{}:main", input.session_id);
    let mut raw_envelopes = Vec::new();
    let mut turns = Vec::new();
    let mut parent_turn_id: Option<String> = None;

    for (idx, msg) in input.messages.iter().enumerate() {
        let source_entry_id = format!("message:{idx}");
        let payload = serde_json::to_value(msg)?;
        let payload_sha256 = Some(stable_json_sha256(&payload));
        let raw_id = derive_raw_id(
            input.session_id,
            input.source_kind,
            input.source_session_id,
            idx as u64,
            payload_sha256.as_deref(),
        );
        raw_envelopes.push(RawEnvelope {
            raw_id: raw_id.clone(),
            source: input.source_kind.to_string(),
            harness: "pi".to_string(),
            native_type: "agent_message".to_string(),
            source_sequence: idx as u64,
            received_at: timestamp_i64(msg.timestamp),
            native_schema_version: None,
            payload: payload.clone(),
            payload_sha256: payload_sha256.clone(),
            extensions: None,
        });

        let role = normalize_role(&msg.role);
        let turn_version = (turns.len() + 1) as i64;
        let turn_id = derive_turn_id(&TurnIdInput {
            session_id: input.session_id,
            branch_id: &branch_id,
            parent_turn_id: parent_turn_id.as_deref(),
            turn_version,
            role: role_to_str(role),
            source_kind: Some(input.source_kind),
            source_session_id: Some(input.source_session_id),
            source_entry_id: Some(&source_entry_id),
            source_hash: payload_sha256.as_deref(),
        });
        let message = project_message(&turn_id, msg, role, &raw_id, idx as i64)?;
        let turn = TimelineTurn {
            turn_id: turn_id.clone(),
            session_id: input.session_id.to_string(),
            branch_id: branch_id.clone(),
            parent_turn_id: parent_turn_id.clone(),
            turn_version: turn_version as u64,
            role,
            status: if msg.is_error.unwrap_or(false) {
                TurnStatus::Failed
            } else {
                TurnStatus::Committed
            },
            stop_reason: None,
            usage: None,
            messages: vec![message],
            raw_refs: vec![raw_id],
            context_snapshot: None,
            created_at: timestamp_i64(msg.timestamp),
            committed_at: msg.timestamp.map(timestamp_u64_to_i64),
            source: Some(TimelineSource {
                source_kind: input.source_kind.to_string(),
                source_session_id: input.source_session_id.to_string(),
                source_entry_id: Some(source_entry_id),
                source_hash: payload_sha256,
                source_timestamp: msg.timestamp.map(timestamp_u64_to_i64),
            }),
            extensions: None,
        };
        parent_turn_id = Some(turn_id);
        turns.push(turn);
    }

    let now = input
        .messages
        .last()
        .and_then(|msg| msg.timestamp)
        .map(timestamp_u64_to_i64)
        .unwrap_or(0);
    let document = TimelineDocument {
        schema_version: oqto_protocol::timeline::TIMELINE_SCHEMA_VERSION,
        session: TimelineSession {
            session_id: input.session_id.to_string(),
            platform_id: input.platform_id.to_string(),
            external_id: input.external_id.map(ToString::to_string),
            user_id: input.user_id.to_string(),
            workspace_id: input.workspace_id.map(ToString::to_string),
            created_at: input
                .messages
                .first()
                .and_then(|msg| msg.timestamp)
                .map(timestamp_u64_to_i64)
                .unwrap_or(now),
            updated_at: now,
            extensions: None,
        },
        branches: vec![TimelineBranch {
            branch_id,
            session_id: input.session_id.to_string(),
            parent_branch_id: None,
            forked_from_turn_id: None,
            head_turn_id: parent_turn_id,
            created_at: input
                .messages
                .first()
                .and_then(|msg| msg.timestamp)
                .map(timestamp_u64_to_i64)
                .unwrap_or(now),
            extensions: None,
        }],
        turns,
        raw_envelopes,
        artifacts: vec![],
        extensions: None,
    };
    validate_timeline(&document)?;
    Ok(document)
}

fn project_message(
    turn_id: &str,
    msg: &AgentMessage,
    role: Role,
    raw_id: &str,
    source_idx: i64,
) -> Result<TimelineMessage> {
    let content_text = extract_text(&msg.content);
    let message_id = derive_message_id(&MessageIdInput {
        turn_id,
        seq: 0,
        kind: "message",
        role: Some(role_to_str(role)),
        source_message_id: Some(&format!("message:{source_idx}")),
        content: Some(&content_text),
    });
    let parts = project_parts(&message_id, msg, raw_id)?;
    Ok(TimelineMessage {
        message_id,
        seq: 0,
        role,
        parts,
        created_at: timestamp_i64(msg.timestamp),
        completed_at: msg.timestamp.map(timestamp_u64_to_i64),
        source_message_id: Some(format!("message:{source_idx}")),
        extensions: None,
    })
}

fn project_parts(message_id: &str, msg: &AgentMessage, raw_id: &str) -> Result<Vec<TimelinePart>> {
    if let Some(tool_call_id) = msg.tool_call_id.as_deref()
        && normalize_role(&msg.role) == Role::Tool
    {
        return Ok(vec![TimelinePart::ToolResult {
            part_id: format!("{message_id}:part:0"),
            seq: 0,
            tool_call_id: tool_call_id.to_string(),
            name: msg.tool_name.clone(),
            result: Some(msg.content.clone()),
            is_error: msg.is_error.unwrap_or(false),
            status: if msg.is_error.unwrap_or(false) {
                ToolLifecycleStatus::Failed
            } else {
                ToolLifecycleStatus::Completed
            },
            error: msg
                .is_error
                .unwrap_or(false)
                .then(|| oqto_protocol::timeline::ToolError {
                    message: extract_text(&msg.content),
                    code: None,
                    details: Some(msg.content.clone()),
                }),
            started_at: None,
            completed_at: msg.timestamp.map(timestamp_u64_to_i64),
            raw_refs: vec![raw_id.to_string()],
            extensions: None,
        }]);
    }

    if let Value::Array(items) = &msg.content {
        let mut parts = Vec::new();
        for (seq, item) in items.iter().enumerate() {
            if let Some(part) = project_array_item(message_id, seq as u32, item, raw_id, msg)? {
                parts.push(part);
            }
        }
        if !parts.is_empty() {
            return Ok(parts);
        }
    }

    Ok(vec![TimelinePart::Content {
        part_id: format!("{message_id}:part:0"),
        seq: 0,
        part: oqto_protocol::Part::Text {
            id: format!("{message_id}:part:0"),
            text: extract_text(&msg.content),
            format: None,
        },
        raw_refs: vec![raw_id.to_string()],
        extensions: None,
    }])
}

fn project_array_item(
    message_id: &str,
    seq: u32,
    item: &Value,
    raw_id: &str,
    msg: &AgentMessage,
) -> Result<Option<TimelinePart>> {
    let Some(obj) = item.as_object() else {
        return Ok(Some(TimelinePart::Content {
            part_id: format!("{message_id}:part:{seq}"),
            seq,
            part: oqto_protocol::Part::Text {
                id: format!("{message_id}:part:{seq}"),
                text: item.to_string(),
                format: None,
            },
            raw_refs: vec![raw_id.to_string()],
            extensions: None,
        }));
    };
    let kind = obj
        .get("type")
        .or_else(|| obj.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("text");
    match kind {
        "text" => Ok(Some(TimelinePart::Content {
            part_id: format!("{message_id}:part:{seq}"),
            seq,
            part: oqto_protocol::Part::Text {
                id: format!("{message_id}:part:{seq}"),
                text: obj
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                format: None,
            },
            raw_refs: vec![raw_id.to_string()],
            extensions: None,
        })),
        "thinking" | "reasoning" => Ok(Some(TimelinePart::Content {
            part_id: format!("{message_id}:part:{seq}"),
            seq,
            part: oqto_protocol::Part::Thinking {
                id: format!("{message_id}:part:{seq}"),
                text: obj
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            },
            raw_refs: vec![raw_id.to_string()],
            extensions: None,
        })),
        "tool_use" | "tool_call" | "toolCall" => {
            let tool_call_id = obj
                .get("id")
                .or_else(|| obj.get("tool_call_id"))
                .or_else(|| obj.get("toolCallId"))
                .and_then(Value::as_str)
                .unwrap_or("unknown-tool-call");
            Ok(Some(TimelinePart::ToolCall {
                part_id: format!("{message_id}:part:{seq}"),
                seq,
                tool_call_id: tool_call_id.to_string(),
                name: obj
                    .get("name")
                    .and_then(Value::as_str)
                    .or(msg.tool_name.as_deref())
                    .unwrap_or("tool")
                    .to_string(),
                title: obj
                    .get("title")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                arguments: obj.get("input").or_else(|| obj.get("arguments")).cloned(),
                status: ToolLifecycleStatus::Completed,
                started_at: None,
                completed_at: msg.timestamp.map(timestamp_u64_to_i64),
                raw_refs: vec![raw_id.to_string()],
                extensions: None,
            }))
        }
        "tool_result" | "toolResult" => {
            let tool_call_id = obj
                .get("tool_call_id")
                .or_else(|| obj.get("toolCallId"))
                .or_else(|| obj.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("unknown-tool-call");
            Ok(Some(TimelinePart::ToolResult {
                part_id: format!("{message_id}:part:{seq}"),
                seq,
                tool_call_id: tool_call_id.to_string(),
                name: msg.tool_name.clone(),
                result: obj.get("content").or_else(|| obj.get("output")).cloned(),
                is_error: obj
                    .get("is_error")
                    .or_else(|| obj.get("isError"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                status: ToolLifecycleStatus::Completed,
                error: None,
                started_at: None,
                completed_at: msg.timestamp.map(timestamp_u64_to_i64),
                raw_refs: vec![raw_id.to_string()],
                extensions: None,
            }))
        }
        _ => Ok(Some(TimelinePart::Content {
            part_id: format!("{message_id}:part:{seq}"),
            seq,
            part: oqto_protocol::Part::Text {
                id: format!("{message_id}:part:{seq}"),
                text: item.to_string(),
                format: None,
            },
            raw_refs: vec![raw_id.to_string()],
            extensions: None,
        })),
    }
}

fn timestamp_i64(timestamp: Option<u64>) -> i64 {
    timestamp.map(timestamp_u64_to_i64).unwrap_or(0)
}

fn timestamp_u64_to_i64(timestamp: u64) -> i64 {
    timestamp.min(i64::MAX as u64) as i64
}

fn normalize_role(role: &str) -> Role {
    if role.eq_ignore_ascii_case("user") {
        Role::User
    } else if role.eq_ignore_ascii_case("system") {
        Role::System
    } else if role.eq_ignore_ascii_case("tool") || role.eq_ignore_ascii_case("toolresult") {
        Role::Tool
    } else {
        Role::Assistant
    }
}

fn role_to_str(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
        Role::Tool => "tool",
    }
}

fn extract_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => map
            .get("text")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| content.to_string()),
        _ => content.to_string(),
    }
}

fn stable_json_sha256(value: &Value) -> String {
    use sha2::{Digest, Sha256};
    let canonical = serde_json::to_string(value).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn derive_raw_id(
    session_id: &str,
    source_kind: &str,
    source_session_id: &str,
    source_sequence: u64,
    payload_sha256: Option<&str>,
) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(
        format!(
            "v1|{session_id}|{source_kind}|{source_session_id}|{source_sequence}|{}",
            payload_sha256.unwrap_or("")
        )
        .as_bytes(),
    );
    format!("raw:{}", hex::encode(&hasher.finalize()[..16]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: Value) -> AgentMessage {
        AgentMessage {
            role: role.to_string(),
            content,
            timestamp: Some(1),
            tool_call_id: None,
            tool_name: None,
            is_error: None,
            api: None,
            provider: None,
            model: None,
            usage: None,
            stop_reason: None,
            extra: std::collections::HashMap::new(),
        }
    }

    fn input(messages: &[AgentMessage]) -> PiTimelineProjectionInput<'_> {
        PiTimelineProjectionInput {
            session_id: "ses",
            platform_id: "platform",
            external_id: Some("pi"),
            user_id: "user",
            workspace_id: Some("workspace"),
            source_session_id: "pi",
            source_kind: "pi_jsonl",
            messages,
        }
    }

    #[test]
    fn pi_projection_is_deterministic() {
        let messages = vec![msg("user", Value::String("hello".to_string()))];
        let a = project_pi_messages_to_timeline(&input(&messages)).expect("project a");
        let b = project_pi_messages_to_timeline(&input(&messages)).expect("project b");
        assert_eq!(
            serde_json::to_value(a).unwrap(),
            serde_json::to_value(b).unwrap()
        );
    }

    #[test]
    fn pi_projection_splits_tool_call_parts() {
        let messages = vec![msg(
            "assistant",
            serde_json::json!([
                {"type":"text","text":"I'll run it"},
                {"type":"tool_use","id":"call-1","name":"bash","input":{"command":"true"}}
            ]),
        )];
        let doc = project_pi_messages_to_timeline(&input(&messages)).expect("project");
        assert!(matches!(
            doc.turns[0].messages[0].parts[1],
            TimelinePart::ToolCall { ref tool_call_id, .. } if tool_call_id == "call-1"
        ));
        assert_eq!(doc.raw_envelopes.len(), 1);
    }

    #[test]
    fn pi_projection_models_tool_result() {
        let mut tool = msg("tool", serde_json::json!({"stdout":"ok"}));
        tool.tool_call_id = Some("call-1".to_string());
        tool.tool_name = Some("bash".to_string());
        let messages = vec![
            msg(
                "assistant",
                serde_json::json!([{ "type":"tool_use", "id":"call-1", "name":"bash" }]),
            ),
            tool,
        ];
        let doc = project_pi_messages_to_timeline(&input(&messages)).expect("project");
        assert!(matches!(
            doc.turns[1].messages[0].parts[0],
            TimelinePart::ToolResult { ref tool_call_id, .. } if tool_call_id == "call-1"
        ));
    }
}
