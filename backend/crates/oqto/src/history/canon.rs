//! Canonical chat history conversions.

use hstry_core::parts::{Part, ToolStatus};
use oqto_protocol::messages::{Message, Role, Usage};
use serde_json::Value;

use super::models::{ChatMessage, ChatMessagePart};

pub fn legacy_messages_to_canon(messages: Vec<ChatMessage>) -> Vec<Message> {
    messages
        .into_iter()
        .enumerate()
        .map(|(idx, message)| legacy_message_to_canon(message, idx as u32))
        .collect()
}

pub fn legacy_message_to_canon(message: ChatMessage, idx: u32) -> Message {
    let role = parse_role(&message.role);
    let parts: Vec<Part> = message
        .parts
        .into_iter()
        .filter_map(legacy_part_to_canon)
        .collect();

    let usage = if message.tokens_input.is_some()
        || message.tokens_output.is_some()
        || message.cost.is_some()
    {
        Some(Usage {
            input_tokens: message.tokens_input.unwrap_or(0).max(0) as u64,
            output_tokens: message.tokens_output.unwrap_or(0).max(0) as u64,
            cache_read_tokens: None,
            cache_write_tokens: None,
            cost_usd: message.cost,
        })
    } else {
        None
    };

    let (tool_call_id, tool_name, is_error) = if role == Role::Tool {
        parts
            .iter()
            .find_map(tool_result_metadata)
            .unwrap_or((None, None, None))
    } else {
        (None, None, None)
    };

    Message {
        id: message.id,
        idx,
        role,
        client_id: message.client_id,
        sender: None,
        parts,
        created_at: message.created_at,
        model: message.model_id,
        provider: message.provider_id,
        stop_reason: None,
        usage,
        tool_call_id,
        tool_name,
        is_error,
        metadata: None,
    }
}

fn parse_role(role: &str) -> Role {
    match role.to_lowercase().as_str() {
        "user" | "human" => Role::User,
        "assistant" | "agent" | "ai" | "bot" => Role::Assistant,
        "system" => Role::System,
        "tool" | "function" | "toolresult" | "tool_result" => Role::Tool,
        _ => Role::User,
    }
}

fn legacy_part_to_canon(part: ChatMessagePart) -> Option<Part> {
    match part.part_type.as_str() {
        "text" => Some(Part::Text {
            id: part.id,
            text: part.text.unwrap_or_default(),
            format: None,
        }),
        "thinking" => Some(Part::Thinking {
            id: part.id,
            text: part.text.unwrap_or_default(),
        }),
        "tool_call" => {
            let id = part.id;
            let tool_call_id = part.tool_call_id.clone().unwrap_or_else(|| id.clone());
            Some(Part::ToolCall {
                id,
                tool_call_id,
                name: part.tool_name.unwrap_or_else(|| "tool".to_string()),
                input: part.tool_input,
                status: part
                    .tool_status
                    .as_deref()
                    .map(ToolStatus::parse)
                    .unwrap_or_default(),
            })
        }
        "tool_result" => {
            let id = part.id;
            let tool_call_id = part.tool_call_id.clone().unwrap_or_else(|| id.clone());
            Some(Part::ToolResult {
                id,
                tool_call_id,
                name: part.tool_name,
                output: part.tool_output.as_deref().map(parse_tool_output),
                is_error: part
                    .tool_status
                    .as_deref()
                    .map(|status| ToolStatus::parse(status) == ToolStatus::Error)
                    .unwrap_or(false),
                duration_ms: None,
            })
        }
        _ => part.text.map(|text| Part::Text {
            id: part.id,
            text,
            format: None,
        }),
    }
}

fn parse_tool_output(output: &str) -> Value {
    serde_json::from_str(output).unwrap_or_else(|_| Value::String(output.to_owned()))
}

fn tool_result_metadata(part: &Part) -> Option<(Option<String>, Option<String>, Option<bool>)> {
    match part {
        Part::ToolResult {
            tool_call_id,
            name,
            is_error,
            ..
        } => Some((Some(tool_call_id.clone()), name.clone(), Some(*is_error))),
        _ => None,
    }
}
