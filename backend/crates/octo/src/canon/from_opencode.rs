//! Converter from OpenCode/Claude Code format to canonical format.
//!
//! OpenCode stores sessions and messages in a directory-based structure:
//! - Sessions: ~/.local/share/opencode/storage/session/{session_id}.json
//! - Messages: ~/.local/share/opencode/storage/message/{session_id}/{msg_id}.json
//! - Parts: ~/.local/share/opencode/storage/part/{msg_id}/{part_id}.json

use serde_json::Value;

use crate::history::models::{ChatMessage, ChatMessagePart, ChatSession, MessageInfo, PartInfo};

use super::{
    CanonConversation, CanonMessage, CanonPart, MessageRole, ModelInfo,
    TokenUsage as CanonTokenUsage, ToolStatus,
};

// ============================================================================
// Session/Conversation Conversion
// ============================================================================

/// Convert a ChatSession (from history models) to CanonConversation.
pub fn opencode_session_to_conversation(session: &ChatSession) -> CanonConversation {
    CanonConversation {
        id: session.id.clone(),
        external_id: Some(session.id.clone()),
        readable_id: Some(session.readable_id.clone()),
        title: session.title.clone(),
        workspace: Some(session.workspace_path.clone()),
        project_name: Some(session.project_name.clone()),
        created_at: session.created_at,
        updated_at: session.updated_at,
        model: None, // OpenCode doesn't store primary model at session level
        tokens_in: None,
        tokens_out: None,
        cost_usd: None,
        parent_id: session.parent_id.clone(),
        agent: Some("opencode".to_string()),
        is_active: false,
        messages: Vec::new(),
        metadata: session.source_path.as_ref().map(|p| {
            serde_json::json!({
                "sourcePath": p,
                "version": session.version
            })
        }),
    }
}

// ============================================================================
// Message Conversion
// ============================================================================

/// Convert a ChatMessage (from history models) to CanonMessage.
pub fn opencode_message_to_canon(msg: &ChatMessage) -> CanonMessage {
    let role = MessageRole::parse(&msg.role);

    let mut canon = CanonMessage {
        id: msg.id.clone(),
        session_id: msg.session_id.clone(),
        role,
        content: String::new(),
        parts: Vec::new(),
        created_at: msg.created_at,
        completed_at: msg.completed_at,
        model: model_from_ids(&msg.provider_id, &msg.model_id),
        tokens: tokens_from_msg(msg),
        cost_usd: msg.cost,
        parent_id: msg.parent_id.clone(),
        agent: msg.agent.clone().or_else(|| Some("opencode".to_string())),
        metadata: None,
    };

    // Convert parts
    for part in &msg.parts {
        if let Some(canon_part) = opencode_part_to_canon(part) {
            // Accumulate text content
            if let Some(text) = canon_part.text_content() {
                if !canon.content.is_empty() {
                    canon.content.push_str("\n\n");
                }
                canon.content.push_str(text);
            }
            canon.parts.push(canon_part);
        }
    }

    canon
}

/// Convert a MessageInfo (raw OpenCode format) to CanonMessage.
pub fn opencode_message_info_to_canon(info: &MessageInfo, parts: &[PartInfo]) -> CanonMessage {
    let role = MessageRole::parse(&info.role);

    let mut canon = CanonMessage {
        id: info.id.clone(),
        session_id: info.session_id.clone(),
        role,
        content: String::new(),
        parts: Vec::new(),
        created_at: info.time.created,
        completed_at: info.time.completed,
        model: model_from_ids(&info.provider_id, &info.model_id),
        tokens: info.tokens.as_ref().map(|t| CanonTokenUsage {
            input: t.input,
            output: t.output,
            reasoning: t.reasoning,
            cache_read: None,
            cache_write: None,
        }),
        cost_usd: info.cost,
        parent_id: info.parent_id.clone(),
        agent: info.agent.clone().or_else(|| Some("opencode".to_string())),
        metadata: None,
    };

    // Convert parts
    for part in parts {
        if let Some(canon_part) = opencode_part_info_to_canon(part) {
            if let Some(text) = canon_part.text_content() {
                if !canon.content.is_empty() {
                    canon.content.push_str("\n\n");
                }
                canon.content.push_str(text);
            }
            canon.parts.push(canon_part);
        }
    }

    canon
}

fn model_from_ids(provider_id: &Option<String>, model_id: &Option<String>) -> Option<ModelInfo> {
    match (provider_id, model_id) {
        (Some(provider), Some(model)) => Some(ModelInfo::new(provider, model)),
        (None, Some(model)) => Some(ModelInfo::new("unknown", model)),
        _ => None,
    }
}

fn tokens_from_msg(msg: &ChatMessage) -> Option<CanonTokenUsage> {
    if msg.tokens_input.is_some() || msg.tokens_output.is_some() {
        Some(CanonTokenUsage {
            input: msg.tokens_input,
            output: msg.tokens_output,
            reasoning: msg.tokens_reasoning,
            cache_read: None,
            cache_write: None,
        })
    } else {
        None
    }
}

// ============================================================================
// Part Conversion
// ============================================================================

/// Convert a ChatMessagePart to CanonPart.
pub fn opencode_part_to_canon(part: &ChatMessagePart) -> Option<CanonPart> {
    match part.part_type.as_str() {
        "text" => {
            let text = part.text.clone().unwrap_or_default();
            Some(CanonPart::Text {
                id: part.id.clone(),
                text,
                format: super::TextFormat::Markdown,
                meta: None,
            })
        }

        "thinking" => {
            let text = part.text.clone().unwrap_or_default();
            Some(CanonPart::Thinking {
                id: part.id.clone(),
                text,
                visibility: super::ThinkingVisibility::Ui,
                meta: None,
            })
        }

        "tool" => {
            // OpenCode uses "tool" for both tool calls and results
            // We can distinguish by the presence of output
            if part.tool_output.is_some() {
                // This is a tool result
                Some(CanonPart::ToolResult {
                    id: part.id.clone(),
                    tool_call_id: part.id.clone(), // OpenCode uses same ID
                    name: part.tool_name.clone(),
                    output: part.tool_output.as_ref().map(|o| Value::String(o.clone())),
                    is_error: part.tool_status.as_deref() == Some("error"),
                    title: part.tool_title.clone(),
                    duration_ms: None,
                    meta: None,
                })
            } else {
                // This is a tool call
                Some(CanonPart::ToolCall {
                    id: part.id.clone(),
                    tool_call_id: part.id.clone(),
                    name: part
                        .tool_name
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                    input: part.tool_input.clone(),
                    status: part
                        .tool_status
                        .as_ref()
                        .map(|s| ToolStatus::parse(s))
                        .unwrap_or(ToolStatus::Pending),
                    meta: None,
                })
            }
        }

        "step-start" | "step-finish" => {
            // These are OpenCode-specific markers, convert to extension
            Some(CanonPart::Extension {
                part_type: format!("x-opencode-{}", part.part_type),
                id: part.id.clone(),
                payload: None,
                meta: None,
            })
        }

        _ => {
            // Unknown part type - use extension
            Some(CanonPart::Extension {
                part_type: format!("x-opencode-{}", part.part_type),
                id: part.id.clone(),
                payload: part.text.as_ref().map(|t| Value::String(t.clone())),
                meta: None,
            })
        }
    }
}

/// Convert a PartInfo (raw OpenCode format) to CanonPart.
pub fn opencode_part_info_to_canon(part: &PartInfo) -> Option<CanonPart> {
    match part.part_type.as_str() {
        "text" => {
            let text = part.text.clone().unwrap_or_default();
            Some(CanonPart::Text {
                id: part.id.clone(),
                text,
                format: super::TextFormat::Markdown,
                meta: None,
            })
        }

        "thinking" => {
            let text = part.text.clone().unwrap_or_default();
            Some(CanonPart::Thinking {
                id: part.id.clone(),
                text,
                visibility: super::ThinkingVisibility::Ui,
                meta: None,
            })
        }

        "tool" => {
            let state = part.state.as_ref();
            let has_output = state.map(|s| s.output.is_some()).unwrap_or(false);

            if has_output {
                let s = state.unwrap();
                Some(CanonPart::ToolResult {
                    id: part.id.clone(),
                    tool_call_id: part.id.clone(),
                    name: part.tool.clone(),
                    output: s.output.as_ref().map(|o| Value::String(o.clone())),
                    is_error: s.status.as_deref() == Some("error"),
                    title: s.title.clone(),
                    duration_ms: None,
                    meta: None,
                })
            } else {
                Some(CanonPart::ToolCall {
                    id: part.id.clone(),
                    tool_call_id: part.id.clone(),
                    name: part.tool.clone().unwrap_or_else(|| "unknown".to_string()),
                    input: state.and_then(|s| s.input.clone()),
                    status: state
                        .and_then(|s| s.status.as_ref())
                        .map(|s| ToolStatus::parse(s))
                        .unwrap_or(ToolStatus::Pending),
                    meta: None,
                })
            }
        }

        _ => {
            // Unknown part type
            Some(CanonPart::Extension {
                part_type: format!("x-opencode-{}", part.part_type),
                id: part.id.clone(),
                payload: part.text.as_ref().map(|t| Value::String(t.clone())),
                meta: None,
            })
        }
    }
}

// ============================================================================
// agent_rpc types conversion
// ============================================================================

/// Convert agent_rpc::Message to CanonMessage.
pub fn agent_rpc_message_to_canon(msg: &crate::agent_rpc::Message) -> CanonMessage {
    let role = MessageRole::parse(&msg.role);

    let mut canon = CanonMessage {
        id: msg.id.clone(),
        session_id: msg.session_id.clone(),
        role,
        content: String::new(),
        parts: Vec::new(),
        created_at: msg.created_at,
        completed_at: msg.completed_at,
        model: msg
            .model
            .as_ref()
            .map(|m| ModelInfo::new(&m.provider_id, &m.model_id)),
        tokens: msg.tokens.as_ref().map(|t| CanonTokenUsage {
            input: t.input,
            output: t.output,
            reasoning: t.reasoning,
            cache_read: t.cache.as_ref().and_then(|c| c.read),
            cache_write: t.cache.as_ref().and_then(|c| c.write),
        }),
        cost_usd: None,
        parent_id: None,
        agent: Some("opencode".to_string()),
        metadata: None,
    };

    // Convert parts
    for part in &msg.parts {
        if let Some(canon_part) = agent_rpc_part_to_canon(part) {
            if let Some(text) = canon_part.text_content() {
                if !canon.content.is_empty() {
                    canon.content.push_str("\n\n");
                }
                canon.content.push_str(text);
            }
            canon.parts.push(canon_part);
        }
    }

    canon
}

fn agent_rpc_part_to_canon(part: &crate::agent_rpc::MessagePart) -> Option<CanonPart> {
    use crate::agent_rpc::MessagePart;

    match part {
        MessagePart::Text { text } => Some(CanonPart::text(text)),

        MessagePart::Tool {
            tool,
            call_id,
            state,
        } => {
            let has_output = state.as_ref().map(|s| s.output.is_some()).unwrap_or(false);
            let part_id = call_id
                .clone()
                .unwrap_or_else(|| format!("part_{}", uuid::Uuid::new_v4().simple()));

            if has_output {
                let s = state.as_ref().unwrap();
                Some(CanonPart::ToolResult {
                    id: part_id.clone(),
                    tool_call_id: part_id,
                    name: Some(tool.clone()),
                    output: s.output.as_ref().map(|o| Value::String(o.clone())),
                    is_error: s.status.as_deref() == Some("error"),
                    title: s.title.clone(),
                    duration_ms: None,
                    meta: s.metadata.clone(),
                })
            } else {
                Some(CanonPart::ToolCall {
                    id: part_id.clone(),
                    tool_call_id: part_id,
                    name: tool.clone(),
                    input: state.as_ref().and_then(|s| s.input.clone()),
                    status: state
                        .as_ref()
                        .and_then(|s| s.status.as_ref())
                        .map(|s| ToolStatus::parse(s))
                        .unwrap_or(ToolStatus::Pending),
                    meta: state.as_ref().and_then(|s| s.metadata.clone()),
                })
            }
        }

        MessagePart::StepStart => Some(CanonPart::Extension {
            part_type: "x-opencode-step-start".to_string(),
            id: format!("part_{}", uuid::Uuid::new_v4().simple()),
            payload: None,
            meta: None,
        }),

        MessagePart::StepFinish {
            reason,
            cost,
            tokens,
        } => Some(CanonPart::Extension {
            part_type: "x-opencode-step-finish".to_string(),
            id: format!("part_{}", uuid::Uuid::new_v4().simple()),
            payload: Some(serde_json::json!({
                "reason": reason,
                "cost": cost,
                "tokens": tokens
            })),
            meta: None,
        }),

        MessagePart::Unknown => None,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_to_conversation() {
        let session = ChatSession {
            id: "ses_123".to_string(),
            readable_id: "amber-builds-beacon".to_string(),
            title: Some("Test Session".to_string()),
            parent_id: None,
            workspace_path: "/home/user/project".to_string(),
            project_name: "project".to_string(),
            created_at: 1700000000000,
            updated_at: 1700000001000,
            version: Some("1.0".to_string()),
            is_child: false,
            source_path: Some("/path/to/session.json".to_string()),
        };

        let conv = opencode_session_to_conversation(&session);

        assert_eq!(conv.id, "ses_123");
        assert_eq!(conv.external_id, Some("ses_123".to_string()));
        assert_eq!(conv.readable_id, Some("amber-builds-beacon".to_string()));
        assert_eq!(conv.title, Some("Test Session".to_string()));
        assert_eq!(conv.workspace, Some("/home/user/project".to_string()));
        assert_eq!(conv.agent, Some("opencode".to_string()));
    }

    #[test]
    fn test_message_to_canon() {
        let msg = ChatMessage {
            id: "msg_001".to_string(),
            session_id: "ses_123".to_string(),
            role: "assistant".to_string(),
            created_at: 1700000000000,
            completed_at: Some(1700000001000),
            parent_id: None,
            model_id: Some("claude-3-5-sonnet".to_string()),
            provider_id: Some("anthropic".to_string()),
            agent: Some("coder".to_string()),
            summary_title: None,
            tokens_input: Some(100),
            tokens_output: Some(50),
            tokens_reasoning: None,
            cost: Some(0.001),
            parts: vec![ChatMessagePart {
                id: "part_001".to_string(),
                part_type: "text".to_string(),
                text: Some("Hello!".to_string()),
                text_html: None,
                tool_name: None,
                tool_input: None,
                tool_output: None,
                tool_status: None,
                tool_title: None,
            }],
        };

        let canon = opencode_message_to_canon(&msg);

        assert_eq!(canon.id, "msg_001");
        assert_eq!(canon.role, MessageRole::Assistant);
        assert_eq!(canon.content, "Hello!");
        assert_eq!(canon.parts.len(), 1);
        assert!(canon.model.is_some());
        assert_eq!(canon.agent, Some("coder".to_string()));
    }

    #[test]
    fn test_tool_part_conversion() {
        // Tool call (no output)
        let call_part = ChatMessagePart {
            id: "tool_001".to_string(),
            part_type: "tool".to_string(),
            text: None,
            text_html: None,
            tool_name: Some("bash".to_string()),
            tool_input: Some(serde_json::json!({"command": "ls"})),
            tool_output: None,
            tool_status: Some("running".to_string()),
            tool_title: None,
        };

        let canon_call = opencode_part_to_canon(&call_part).unwrap();
        match canon_call {
            CanonPart::ToolCall { name, status, .. } => {
                assert_eq!(name, "bash");
                assert_eq!(status, ToolStatus::Running);
            }
            _ => panic!("Expected ToolCall"),
        }

        // Tool result (has output)
        let result_part = ChatMessagePart {
            id: "tool_001".to_string(),
            part_type: "tool".to_string(),
            text: None,
            text_html: None,
            tool_name: Some("bash".to_string()),
            tool_input: Some(serde_json::json!({"command": "ls"})),
            tool_output: Some("file1\nfile2".to_string()),
            tool_status: Some("success".to_string()),
            tool_title: Some("List files".to_string()),
        };

        let canon_result = opencode_part_to_canon(&result_part).unwrap();
        match canon_result {
            CanonPart::ToolResult {
                name,
                is_error,
                title,
                ..
            } => {
                assert_eq!(name, Some("bash".to_string()));
                assert!(!is_error);
                assert_eq!(title, Some("List files".to_string()));
            }
            _ => panic!("Expected ToolResult"),
        }
    }
}
