//! Converter from Pi agent format to canonical format.
//!
//! Pi uses JSONL files with a specific message format. This module converts
//! Pi's types to the canonical format for unified handling.

use serde_json::{Value, json};

use crate::pi::{
    AgentMessage, AssistantMessageEvent, ContentBlock, PiEvent, PiState, SessionStats, TokenUsage,
};

use super::{
    CanonConversation, CanonMessage, CanonPart, MessageRole, ModelInfo, StreamEvent,
    TokenUsage as CanonTokenUsage, ToolStatus,
};

// ============================================================================
// Message Conversion
// ============================================================================

/// Convert a Pi AgentMessage to a CanonMessage.
pub fn pi_message_to_canon(msg: &AgentMessage, session_id: &str) -> CanonMessage {
    let role = MessageRole::parse(&msg.role);
    let timestamp = msg
        .timestamp
        .map(|t| t as i64)
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

    let mut canon = CanonMessage {
        id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
        session_id: session_id.to_string(),
        role,
        content: String::new(),
        parts: Vec::new(),
        created_at: timestamp,
        completed_at: if role == MessageRole::Assistant {
            Some(timestamp)
        } else {
            None
        },
        model: pi_model_info(msg),
        tokens: pi_token_usage(&msg.usage),
        cost_usd: msg
            .usage
            .as_ref()
            .and_then(|u| u.cost.as_ref().map(|c| c.total)),
        parent_id: None,
        agent: Some("pi".to_string()),
        metadata: Some(json!({
            "source": "pi",
            "raw": msg
        })),
    };

    // Convert content to parts
    convert_content_to_parts(msg, &mut canon);

    canon
}

/// Extract model info from Pi message.
fn pi_model_info(msg: &AgentMessage) -> Option<ModelInfo> {
    match (&msg.provider, &msg.model) {
        (Some(provider), Some(model)) => Some(ModelInfo::new(provider, model)),
        (None, Some(model)) => {
            // Try to infer provider from model name
            let provider = infer_provider_from_model(model);
            Some(ModelInfo::new(provider, model))
        }
        _ => None,
    }
}

/// Infer provider from model name.
fn infer_provider_from_model(model: &str) -> &'static str {
    if model.starts_with("claude") || model.contains("anthropic") {
        "anthropic"
    } else if model.starts_with("gpt") || model.starts_with("o1") || model.starts_with("o3") {
        "openai"
    } else if model.starts_with("gemini") {
        "google"
    } else if model.starts_with("llama") || model.starts_with("mistral") {
        "ollama"
    } else {
        "unknown"
    }
}

/// Convert Pi token usage to canonical format.
fn pi_token_usage(usage: &Option<TokenUsage>) -> Option<CanonTokenUsage> {
    usage.as_ref().map(|u| CanonTokenUsage {
        input: Some(u.input as i64),
        output: Some(u.output as i64),
        reasoning: None,
        cache_read: if u.cache_read > 0 {
            Some(u.cache_read as i64)
        } else {
            None
        },
        cache_write: if u.cache_write > 0 {
            Some(u.cache_write as i64)
        } else {
            None
        },
    })
}

/// Convert Pi content (string or array) to canonical parts.
fn convert_content_to_parts(source: &AgentMessage, msg: &mut CanonMessage) {
    match &source.content {
        Value::String(text) => {
            msg.content = text.clone();
            msg.parts.push(CanonPart::text(text));
        }
        Value::Array(blocks) => {
            for block in blocks {
                if let Ok(content_block) = serde_json::from_value::<ContentBlock>(block.clone()) {
                    match content_block {
                        ContentBlock::Text { text } => {
                            if !msg.content.is_empty() {
                                msg.content.push_str("\n\n");
                            }
                            msg.content.push_str(&text);
                            msg.parts.push(CanonPart::text(&text));
                        }
                        ContentBlock::Thinking { thinking } => {
                            msg.parts.push(CanonPart::thinking(&thinking));
                        }
                        ContentBlock::ToolCall(tool_call) => {
                            msg.parts.push(CanonPart::tool_call(
                                &tool_call.id,
                                &tool_call.name,
                                Some(tool_call.arguments.clone()),
                            ));
                        }
                        ContentBlock::Image { source } => {
                            let media_source = match source {
                                crate::pi::ImageSource::Url { url } => super::MediaSource::url(url),
                                crate::pi::ImageSource::Base64 { media_type, data } => {
                                    super::MediaSource::base64(data, media_type)
                                }
                            };
                            msg.parts.push(CanonPart::Image {
                                id: format!("part_{}", uuid::Uuid::new_v4().simple()),
                                source: media_source,
                                alt: None,
                                dimensions: None,
                                meta: None,
                            });
                        }
                    }
                }
            }
        }
        _ => {}
    }

    if msg.role == MessageRole::Tool {
        let tool_call_id = source
            .tool_call_id
            .clone()
            .unwrap_or_else(|| format!("tool_call_{}", uuid::Uuid::new_v4().simple()));
        let output = source.content.clone();
        let text_content = extract_text_from_content(&source.content);
        if !text_content.is_empty() {
            msg.content = text_content;
        }
        msg.parts.push(CanonPart::ToolResult {
            id: format!("part_{}", uuid::Uuid::new_v4().simple()),
            tool_call_id,
            name: source.tool_name.clone(),
            output: Some(output),
            is_error: source.is_error.unwrap_or(false),
            title: None,
            duration_ms: None,
            meta: None,
        });
    }
}

fn extract_text_from_content(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| {
                if let Some(obj) = block.as_object() {
                    if obj.get("type").and_then(|t| t.as_str()) == Some("text") {
                        return obj.get("text").and_then(|t| t.as_str()).map(String::from);
                    }
                }
                None
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
        other => other.to_string(),
    }
}

// ============================================================================
// Event Conversion
// ============================================================================

/// Convert a Pi event to a canonical stream event.
pub fn pi_event_to_stream_event(
    event: &PiEvent,
    session_id: &str,
    current_message_id: &str,
) -> Option<StreamEvent> {
    match event {
        PiEvent::AgentStart => Some(StreamEvent::AgentStart {
            session_id: session_id.to_string(),
        }),

        PiEvent::AgentEnd { .. } => Some(StreamEvent::AgentEnd {
            session_id: session_id.to_string(),
            reason: Some("completed".to_string()),
        }),

        PiEvent::MessageStart { message } => {
            if message.role == "assistant" {
                Some(StreamEvent::MessageStart {
                    session_id: session_id.to_string(),
                    message_id: current_message_id.to_string(),
                    role: MessageRole::Assistant,
                })
            } else {
                None
            }
        }

        PiEvent::MessageUpdate {
            assistant_message_event,
            message,
        } => convert_assistant_event(
            assistant_message_event,
            session_id,
            current_message_id,
            message,
        ),

        PiEvent::MessageEnd { message } => {
            if message.role == "assistant" {
                Some(StreamEvent::MessageEnd {
                    session_id: session_id.to_string(),
                    message_id: current_message_id.to_string(),
                    tokens: pi_token_usage(&message.usage),
                    cost_usd: message
                        .usage
                        .as_ref()
                        .and_then(|u| u.cost.as_ref().map(|c| c.total)),
                })
            } else {
                None
            }
        }

        PiEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            args,
        } => Some(StreamEvent::ToolCallStart {
            session_id: session_id.to_string(),
            message_id: current_message_id.to_string(),
            tool_call_id: tool_call_id.clone(),
            name: tool_name.clone(),
            input: Some(args.clone()),
        }),

        PiEvent::ToolExecutionUpdate {
            tool_call_id,
            tool_name,
            ..
        } => Some(StreamEvent::ToolCallUpdate {
            session_id: session_id.to_string(),
            tool_call_id: tool_call_id.clone(),
            status: ToolStatus::Running,
            title: Some(tool_name.clone()),
        }),

        PiEvent::ToolExecutionEnd {
            tool_call_id,
            result,
            is_error,
            ..
        } => Some(StreamEvent::ToolCallEnd {
            session_id: session_id.to_string(),
            tool_call_id: tool_call_id.clone(),
            output: Some(serde_json::to_value(result).unwrap_or_default()),
            is_error: *is_error,
            duration_ms: None,
        }),

        PiEvent::AutoCompactionStart { .. }
        | PiEvent::AutoCompactionEnd { .. }
        | PiEvent::AutoRetryStart { .. }
        | PiEvent::AutoRetryEnd { .. }
        | PiEvent::HookError { .. }
        | PiEvent::TurnStart
        | PiEvent::TurnEnd { .. }
        | PiEvent::ExtensionUiRequest(_)
        | PiEvent::Unknown => None,
    }
}

/// Convert Pi AssistantMessageEvent to StreamEvent.
fn convert_assistant_event(
    event: &AssistantMessageEvent,
    session_id: &str,
    message_id: &str,
    _message: &AgentMessage,
) -> Option<StreamEvent> {
    match event {
        AssistantMessageEvent::TextDelta { delta, .. } => Some(StreamEvent::TextDelta {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            delta: delta.clone(),
        }),

        AssistantMessageEvent::ThinkingDelta { delta, .. } => Some(StreamEvent::ThinkingDelta {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            delta: delta.clone(),
        }),

        AssistantMessageEvent::ToolcallEnd { tool_call, .. } => Some(StreamEvent::ToolCallStart {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            tool_call_id: tool_call.id.clone(),
            name: tool_call.name.clone(),
            input: Some(tool_call.arguments.clone()),
        }),

        AssistantMessageEvent::Error { reason, .. } => Some(StreamEvent::Error {
            session_id: session_id.to_string(),
            error: reason.clone(),
            code: None,
        }),

        AssistantMessageEvent::Done { reason, .. } => Some(StreamEvent::AgentEnd {
            session_id: session_id.to_string(),
            reason: Some(reason.clone()),
        }),

        // Ignore partial events that don't need separate streaming
        AssistantMessageEvent::Start { .. }
        | AssistantMessageEvent::TextStart { .. }
        | AssistantMessageEvent::TextEnd { .. }
        | AssistantMessageEvent::ThinkingStart { .. }
        | AssistantMessageEvent::ThinkingEnd { .. }
        | AssistantMessageEvent::ToolcallStart { .. }
        | AssistantMessageEvent::ToolcallDelta { .. }
        | AssistantMessageEvent::Unknown => None,
    }
}

// ============================================================================
// Session/Conversation Conversion
// ============================================================================

/// Convert Pi state to a partial CanonConversation.
pub fn pi_state_to_conversation(state: &PiState, workspace: &str) -> CanonConversation {
    let now = chrono::Utc::now().timestamp_millis();

    CanonConversation {
        id: state
            .session_id
            .clone()
            .unwrap_or_else(|| format!("conv_{}", uuid::Uuid::new_v4().simple())),
        external_id: state.session_id.clone(),
        readable_id: None, // Pi generates readable IDs in session names
        title: state.session_name.clone(),
        workspace: Some(workspace.to_string()),
        project_name: std::path::Path::new(workspace)
            .file_name()
            .and_then(|n| n.to_str())
            .map(String::from),
        created_at: now,
        updated_at: now,
        model: state.model.as_ref().map(|m| m.id.clone()),
        tokens_in: None,
        tokens_out: None,
        cost_usd: None,
        parent_id: None,
        agent: Some("pi".to_string()),
        is_active: state.is_streaming,
        messages: Vec::new(),
        metadata: None,
    }
}

/// Update a conversation with session stats.
pub fn update_conversation_from_stats(conv: &mut CanonConversation, stats: &SessionStats) {
    if let Some(ref session_id) = stats.session_id {
        conv.id = session_id.clone();
        conv.external_id = Some(session_id.clone());
    }

    conv.tokens_in = Some(stats.tokens.input as i64);
    conv.tokens_out = Some(stats.tokens.output as i64);
    conv.cost_usd = Some(stats.cost);
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pi_message_text_only() {
        let msg = AgentMessage {
            role: "user".to_string(),
            content: Value::String("Hello, Pi!".to_string()),
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

        let canon = pi_message_to_canon(&msg, "ses_123");

        assert_eq!(canon.role, MessageRole::User);
        assert_eq!(canon.content, "Hello, Pi!");
        assert_eq!(canon.parts.len(), 1);
        assert_eq!(canon.session_id, "ses_123");
        assert_eq!(canon.agent, Some("pi".to_string()));
    }

    #[test]
    fn test_pi_message_with_model() {
        let msg = AgentMessage {
            role: "assistant".to_string(),
            content: Value::String("Hello!".to_string()),
            timestamp: Some(1700000000000),
            tool_call_id: None,
            tool_name: None,
            is_error: None,
            api: Some("anthropic".to_string()),
            provider: Some("anthropic".to_string()),
            model: Some("claude-3-5-sonnet".to_string()),
            usage: Some(TokenUsage {
                input: 100,
                output: 50,
                cache_read: 0,
                cache_write: 0,
                cost: None,
            }),
            stop_reason: Some("stop".to_string()),
            extra: Default::default(),
        };

        let canon = pi_message_to_canon(&msg, "ses_123");

        assert_eq!(canon.role, MessageRole::Assistant);
        assert!(canon.model.is_some());
        let model = canon.model.unwrap();
        assert_eq!(model.provider_id, "anthropic");
        assert_eq!(model.model_id, "claude-3-5-sonnet");

        assert!(canon.tokens.is_some());
        let tokens = canon.tokens.unwrap();
        assert_eq!(tokens.input, Some(100));
        assert_eq!(tokens.output, Some(50));
    }

    #[test]
    fn test_pi_event_to_stream() {
        let event = PiEvent::AgentStart;
        let stream = pi_event_to_stream_event(&event, "ses_123", "msg_456");

        assert!(stream.is_some());
        if let Some(StreamEvent::AgentStart { session_id }) = stream {
            assert_eq!(session_id, "ses_123");
        } else {
            panic!("Expected AgentStart event");
        }
    }

    #[test]
    fn test_infer_provider() {
        assert_eq!(infer_provider_from_model("claude-3-opus"), "anthropic");
        assert_eq!(infer_provider_from_model("gpt-4o"), "openai");
        assert_eq!(infer_provider_from_model("o1-preview"), "openai");
        assert_eq!(infer_provider_from_model("gemini-pro"), "google");
        assert_eq!(infer_provider_from_model("llama-3.1"), "ollama");
        assert_eq!(infer_provider_from_model("unknown-model"), "unknown");
    }
}
