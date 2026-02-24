//! Converter from hstry database format to canonical format.
//!
//! hstry stores conversations and messages in SQLite with the canonical schema.
//! This module provides functions to read directly into our Rust canonical types.

use anyhow::{Context, Result};
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use std::path::Path;

use super::{
    CanonConversation, CanonMessage, CanonPart, MediaSource, MessageRole, ModelInfo, ToolStatus,
    TokenUsage,
};

// ============================================================================
// Database Access
// ============================================================================

/// Open a connection pool to the hstry database.
pub async fn open_hstry_pool(db_path: &Path) -> Result<SqlitePool> {
    let database_url = format!("sqlite://{}?mode=ro", db_path.display());
    let pool = SqlitePool::connect(&database_url)
        .await
        .with_context(|| format!("connecting to hstry database: {}", db_path.display()))?;
    Ok(pool)
}

/// List all conversations from hstry database.
pub async fn list_conversations_from_hstry(db_path: &Path) -> Result<Vec<CanonConversation>> {
    let pool = open_hstry_pool(db_path).await?;

    let rows = sqlx::query(
        r#"
        SELECT 
            c.id, c.external_id, c.readable_id, c.title, c.workspace,
            c.created_at, c.updated_at, c.model, c.tokens_in, c.tokens_out,
            c.cost_usd, c.metadata,
            s.adapter as source_adapter
        FROM conversations c
        LEFT JOIN sources s ON c.source_id = s.id
        ORDER BY c.updated_at DESC
        "#,
    )
    .fetch_all(&pool)
    .await?;

    let mut conversations = Vec::new();
    for row in rows {
        let id: String = row.get("id");
        let external_id: Option<String> = row.get("external_id");
        let readable_id: Option<String> = row.get("readable_id");
        let title: Option<String> = row.get("title");
        let workspace: Option<String> = row.get("workspace");
        let created_at: Option<i64> = row.get("created_at");
        let updated_at: Option<i64> = row.get("updated_at");
        let model: Option<String> = row.get("model");
        let tokens_in: Option<i64> = row.get("tokens_in");
        let tokens_out: Option<i64> = row.get("tokens_out");
        let cost_usd: Option<f64> = row.get("cost_usd");
        let metadata_json: Option<String> = row.get("metadata");
        let source_adapter: Option<String> = row.get("source_adapter");

        let project_name = workspace.as_ref().and_then(|w| {
            std::path::Path::new(w)
                .file_name()
                .and_then(|n| n.to_str())
                .map(String::from)
        });

        conversations.push(CanonConversation {
            id,
            external_id,
            readable_id,
            title,
            workspace,
            project_name,
            created_at: hstry_timestamp_ms(created_at).unwrap_or(0),
            updated_at: hstry_timestamp_ms(updated_at).unwrap_or(0),
            model,
            tokens_in,
            tokens_out,
            cost_usd,
            parent_id: None,
            agent: source_adapter,
            is_active: false,
            messages: Vec::new(),
            metadata: metadata_json.and_then(|s| serde_json::from_str(&s).ok()),
        });
    }

    Ok(conversations)
}

/// Get a single conversation by ID (external_id, readable_id, or internal id).
pub async fn get_conversation_from_hstry(
    session_id: &str,
    db_path: &Path,
) -> Result<Option<CanonConversation>> {
    let pool = open_hstry_pool(db_path).await?;

    let row = sqlx::query(
        r#"
        SELECT 
            c.id, c.external_id, c.readable_id, c.title, c.workspace,
            c.created_at, c.updated_at, c.model, c.tokens_in, c.tokens_out,
            c.cost_usd, c.metadata,
            s.adapter as source_adapter
        FROM conversations c
        LEFT JOIN sources s ON c.source_id = s.id
        WHERE c.external_id = ? OR c.readable_id = ? OR c.id = ?
        LIMIT 1
        "#,
    )
    .bind(session_id)
    .bind(session_id)
    .bind(session_id)
    .fetch_optional(&pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let id: String = row.get("id");
    let external_id: Option<String> = row.get("external_id");
    let readable_id: Option<String> = row.get("readable_id");
    let title: Option<String> = row.get("title");
    let workspace: Option<String> = row.get("workspace");
    let created_at: Option<i64> = row.get("created_at");
    let updated_at: Option<i64> = row.get("updated_at");
    let model: Option<String> = row.get("model");
    let tokens_in: Option<i64> = row.get("tokens_in");
    let tokens_out: Option<i64> = row.get("tokens_out");
    let cost_usd: Option<f64> = row.get("cost_usd");
    let metadata_json: Option<String> = row.get("metadata");
    let source_adapter: Option<String> = row.get("source_adapter");

    let project_name = workspace.as_ref().and_then(|w| {
        std::path::Path::new(w)
            .file_name()
            .and_then(|n| n.to_str())
            .map(String::from)
    });

    Ok(Some(CanonConversation {
        id,
        external_id,
        readable_id,
        title,
        workspace,
        project_name,
        created_at: hstry_timestamp_ms(created_at).unwrap_or(0),
        updated_at: hstry_timestamp_ms(updated_at).unwrap_or(0),
        model,
        tokens_in,
        tokens_out,
        cost_usd,
        parent_id: None,
        agent: source_adapter,
        is_active: false,
        messages: Vec::new(),
        metadata: metadata_json.and_then(|s| serde_json::from_str(&s).ok()),
    }))
}

/// Get messages for a conversation from hstry database.
pub async fn get_messages_from_hstry(
    session_id: &str,
    db_path: &Path,
) -> Result<Vec<CanonMessage>> {
    let pool = open_hstry_pool(db_path).await?;

    // First find the conversation
    let conversation_row = sqlx::query(
        r#"
        SELECT id
        FROM conversations
        WHERE external_id = ? OR readable_id = ? OR id = ?
        LIMIT 1
        "#,
    )
    .bind(session_id)
    .bind(session_id)
    .bind(session_id)
    .fetch_optional(&pool)
    .await?;

    let Some(conversation_row) = conversation_row else {
        return Ok(Vec::new());
    };
    let conversation_id: String = conversation_row.get("id");

    let rows = sqlx::query(
        r#"
        SELECT id, role, content, created_at, model, tokens, cost_usd, parts_json, metadata
        FROM messages
        WHERE conversation_id = ?
        ORDER BY idx
        "#,
    )
    .bind(&conversation_id)
    .fetch_all(&pool)
    .await?;

    let mut messages = Vec::new();
    for row in rows {
        let id: String = row.get("id");
        let role: String = row.get("role");
        let content: String = row.get("content");
        let created_at: Option<i64> = row.get("created_at");
        let model: Option<String> = row.get("model");
        let tokens: Option<i64> = row.get("tokens");
        let cost: Option<f64> = row.get("cost_usd");
        let parts_json: Option<String> = row.get("parts_json");
        let metadata_json: Option<String> = row.get("metadata");

        let mut parts = parse_hstry_parts(parts_json.as_deref(), &content, &id);

        // For tool result messages, strip text parts that duplicate the tool output.
        // hstry may store both a text part and a tool_result part for the same content.
        if role == "tool" || role == "toolResult" {
            let has_tool_result = parts
                .iter()
                .any(|p| matches!(p, CanonPart::ToolResult { .. }));
            if has_tool_result {
                parts.retain(|p| !matches!(p, CanonPart::Text { .. }));
            }
        }

        let model_info = model.as_ref().map(|m| {
            // Try to parse "provider/model" format
            if let Some((provider, model_id)) = m.split_once('/') {
                ModelInfo::new(provider, model_id)
            } else {
                ModelInfo::new("unknown", m)
            }
        });

        messages.push(CanonMessage {
            id,
            session_id: session_id.to_string(),
            role: MessageRole::parse(&role),
            content,
            parts,
            created_at: hstry_timestamp_ms(created_at).unwrap_or(0),
            completed_at: None,
            model: model_info,
            tokens: tokens.map(|t| TokenUsage {
                output: Some(t),
                ..Default::default()
            }),
            cost_usd: cost,
            parent_id: None,
            agent: None,
            metadata: metadata_json.and_then(|s| serde_json::from_str(&s).ok()),
        });
    }

    Ok(messages)
}

// ============================================================================
// Part Parsing
// ============================================================================

/// Parse hstry's parts_json column into canonical parts.
fn parse_hstry_parts(parts_json: Option<&str>, content: &str, message_id: &str) -> Vec<CanonPart> {
    let mut parts = Vec::new();

    if let Some(json_str) = parts_json {
        if let Ok(Value::Array(values)) = serde_json::from_str(json_str) {
            for (idx, value) in values.iter().enumerate() {
                if let Some(part) = parse_single_part(value, message_id, idx) {
                    parts.push(part);
                }
            }
        }
    }

    // Fallback: if no parts parsed, create a text part from content
    if parts.is_empty() && !content.trim().is_empty() {
        parts.push(CanonPart::Text {
            id: format!("{message_id}-part-0"),
            text: content.to_string(),
            format: super::TextFormat::Markdown,
            meta: None,
        });
    }

    parts
}

/// Parse a single part from hstry's JSON format.
fn parse_single_part(value: &Value, message_id: &str, idx: usize) -> Option<CanonPart> {
    let obj = value.as_object()?;
    let part_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("text");
    let part_id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| format!("{message_id}-part-{idx}"));

    match part_type {
        "text" => {
            let text = obj.get("text").and_then(|v| v.as_str())?;
            Some(CanonPart::Text {
                id: part_id,
                text: text.to_string(),
                format: obj
                    .get("format")
                    .and_then(|v| v.as_str())
                    .map(|f| {
                        if f == "plain" {
                            super::TextFormat::Plain
                        } else {
                            super::TextFormat::Markdown
                        }
                    })
                    .unwrap_or(super::TextFormat::Markdown),
                meta: obj.get("meta").cloned(),
            })
        }

        "thinking" => {
            let text = obj.get("text").and_then(|v| v.as_str())?;
            Some(CanonPart::Thinking {
                id: part_id,
                text: text.to_string(),
                visibility: obj
                    .get("visibility")
                    .and_then(|v| v.as_str())
                    .map(|v| {
                        if v == "hidden" {
                            super::ThinkingVisibility::Hidden
                        } else {
                            super::ThinkingVisibility::Ui
                        }
                    })
                    .unwrap_or(super::ThinkingVisibility::Ui),
                meta: obj.get("meta").cloned(),
            })
        }

        "tool_call" => {
            let tool_call_id = obj
                .get("toolCallId")
                .or_else(|| obj.get("tool_call_id"))
                .or_else(|| obj.get("id"))
                .and_then(|v| v.as_str())?
                .to_string();
            let name = obj.get("name").and_then(|v| v.as_str())?.to_string();
            Some(CanonPart::ToolCall {
                id: part_id,
                tool_call_id,
                name,
                input: obj.get("input").or_else(|| obj.get("arguments")).cloned(),
                status: obj
                    .get("status")
                    .and_then(|v| v.as_str())
                    .map(ToolStatus::parse)
                    .unwrap_or(ToolStatus::Pending),
                meta: obj.get("meta").cloned(),
            })
        }

        "tool_result" => {
            let tool_call_id = obj
                .get("toolCallId")
                .or_else(|| obj.get("tool_call_id"))
                .and_then(|v| v.as_str())?
                .to_string();
            Some(CanonPart::ToolResult {
                id: part_id,
                tool_call_id,
                name: obj.get("name").and_then(|v| v.as_str()).map(String::from),
                output: obj.get("output").cloned(),
                is_error: obj.get("isError").and_then(|v| v.as_bool()).unwrap_or(false),
                title: obj.get("title").and_then(|v| v.as_str()).map(String::from),
                duration_ms: obj.get("durationMs").and_then(|v| v.as_u64()),
                meta: obj.get("meta").cloned(),
            })
        }

        "file_ref" => {
            let uri = obj.get("uri").and_then(|v| v.as_str())?.to_string();
            Some(CanonPart::FileRef {
                id: part_id,
                uri,
                label: obj.get("label").and_then(|v| v.as_str()).map(String::from),
                range: parse_file_range(obj.get("range")),
                origin_text: obj
                    .get("originText")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                meta: obj.get("meta").cloned(),
            })
        }

        "citation" => Some(CanonPart::Citation {
            id: part_id,
            label: obj.get("label").and_then(|v| v.as_str()).map(String::from),
            target_uri: obj
                .get("targetUri")
                .and_then(|v| v.as_str())
                .map(String::from),
            target_range: parse_file_range(obj.get("targetRange")),
            origin_text: obj
                .get("originText")
                .and_then(|v| v.as_str())
                .map(String::from),
            meta: obj.get("meta").cloned(),
        }),

        "image" => {
            let source = parse_media_source(obj)?;
            Some(CanonPart::Image {
                id: part_id,
                source,
                alt: obj.get("alt").and_then(|v| v.as_str()).map(String::from),
                dimensions: parse_dimensions(obj),
                meta: obj.get("meta").cloned(),
            })
        }

        "audio" => {
            let source = parse_media_source(obj)?;
            Some(CanonPart::Audio {
                id: part_id,
                source,
                duration_sec: obj.get("durationSec").and_then(|v| v.as_f64()),
                transcript: obj
                    .get("transcript")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                meta: obj.get("meta").cloned(),
            })
        }

        "video" => {
            let source = parse_media_source(obj)?;
            Some(CanonPart::Video {
                id: part_id,
                source,
                duration_sec: obj.get("durationSec").and_then(|v| v.as_f64()),
                dimensions: parse_dimensions(obj),
                thumbnail: obj
                    .get("thumbnail")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                meta: obj.get("meta").cloned(),
            })
        }

        "attachment" => {
            let source = parse_media_source(obj)?;
            Some(CanonPart::Attachment {
                id: part_id,
                source,
                filename: obj
                    .get("filename")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                size_bytes: obj.get("sizeBytes").and_then(|v| v.as_u64()),
                meta: obj.get("meta").cloned(),
            })
        }

        // Extension types (x-*)
        t if t.starts_with("x-") => Some(CanonPart::Extension {
            part_type: t.to_string(),
            id: part_id,
            payload: obj.get("payload").cloned(),
            meta: obj.get("meta").cloned(),
        }),

        // Unknown types - wrap as extension
        _ => Some(CanonPart::Extension {
            part_type: format!("x-unknown-{}", part_type),
            id: part_id,
            payload: Some(value.clone()),
            meta: None,
        }),
    }
}

fn parse_file_range(value: Option<&Value>) -> Option<super::FileRange> {
    let obj = value?.as_object()?;
    Some(super::FileRange {
        start_line: obj.get("startLine").and_then(|v| v.as_u64()).map(|v| v as u32),
        end_line: obj.get("endLine").and_then(|v| v.as_u64()).map(|v| v as u32),
        start_col: obj.get("startCol").and_then(|v| v.as_u64()).map(|v| v as u32),
        end_col: obj.get("endCol").and_then(|v| v.as_u64()).map(|v| v as u32),
    })
}

fn parse_media_source(obj: &serde_json::Map<String, Value>) -> Option<MediaSource> {
    // Try URL first
    if let Some(url) = obj.get("url").and_then(|v| v.as_str()) {
        let mime_type = obj.get("mimeType").and_then(|v| v.as_str()).map(String::from);
        return Some(MediaSource::Url {
            url: url.to_string(),
            mime_type,
        });
    }

    // Try base64
    if let Some(data) = obj.get("data").and_then(|v| v.as_str()) {
        let mime_type = obj
            .get("mimeType")
            .and_then(|v| v.as_str())
            .unwrap_or("application/octet-stream");
        return Some(MediaSource::Base64 {
            data: data.to_string(),
            mime_type: mime_type.to_string(),
        });
    }

    None
}

fn parse_dimensions(obj: &serde_json::Map<String, Value>) -> Option<super::MediaDimensions> {
    let width = obj.get("width").and_then(|v| v.as_u64())? as u32;
    let height = obj.get("height").and_then(|v| v.as_u64())? as u32;
    Some(super::MediaDimensions { width, height })
}

/// Convert hstry timestamp (seconds) to milliseconds.
fn hstry_timestamp_ms(value: Option<i64>) -> Option<i64> {
    value.map(|v| if v < 10_000_000_000 { v * 1000 } else { v })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text_part() {
        let json = r#"{"type": "text", "text": "Hello world", "format": "markdown"}"#;
        let value: Value = serde_json::from_str(json).unwrap();
        let part = parse_single_part(&value, "msg_1", 0).unwrap();

        match part {
            CanonPart::Text { text, format, .. } => {
                assert_eq!(text, "Hello world");
                assert_eq!(format, super::super::TextFormat::Markdown);
            }
            _ => panic!("Expected Text part"),
        }
    }

    #[test]
    fn test_parse_tool_call_part() {
        let json = r#"{
            "type": "tool_call",
            "id": "part_123",
            "toolCallId": "call_456",
            "name": "bash",
            "input": {"command": "ls -la"}
        }"#;
        let value: Value = serde_json::from_str(json).unwrap();
        let part = parse_single_part(&value, "msg_1", 0).unwrap();

        match part {
            CanonPart::ToolCall {
                id,
                tool_call_id,
                name,
                input,
                ..
            } => {
                assert_eq!(id, "part_123");
                assert_eq!(tool_call_id, "call_456");
                assert_eq!(name, "bash");
                assert!(input.is_some());
            }
            _ => panic!("Expected ToolCall part"),
        }
    }

    #[test]
    fn test_parse_image_url_part() {
        let json = r#"{
            "type": "image",
            "url": "https://example.com/image.png",
            "mimeType": "image/png",
            "alt": "Screenshot"
        }"#;
        let value: Value = serde_json::from_str(json).unwrap();
        let part = parse_single_part(&value, "msg_1", 0).unwrap();

        match part {
            CanonPart::Image { source, alt, .. } => {
                assert!(matches!(source, MediaSource::Url { .. }));
                assert_eq!(alt, Some("Screenshot".to_string()));
            }
            _ => panic!("Expected Image part"),
        }
    }

    #[test]
    fn test_hstry_timestamp_conversion() {
        // Seconds -> milliseconds
        assert_eq!(hstry_timestamp_ms(Some(1700000000)), Some(1700000000000));
        // Already milliseconds
        assert_eq!(hstry_timestamp_ms(Some(1700000000000)), Some(1700000000000));
        // None
        assert_eq!(hstry_timestamp_ms(None), None);
    }
}
