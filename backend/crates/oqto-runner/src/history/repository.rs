use std::path::{Path, PathBuf};

use anyhow::Result;
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use crate::history::HstryClient;

#[derive(Debug, Clone)]
pub struct RunnerChatSession {
    pub workspace_path: String,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub parent_id: Option<String>,
    pub model_id: Option<String>,
    pub provider_id: Option<String>,
    pub agent: Option<String>,
    pub summary_title: Option<String>,
    pub tokens_input: Option<i64>,
    pub tokens_output: Option<i64>,
    pub tokens_reasoning: Option<i64>,
    pub cost: Option<f64>,
    pub client_id: Option<String>,
    pub parts: Vec<ChatMessagePart>,
}

#[derive(Debug, Clone)]
pub struct ChatMessagePart {
    pub id: String,
    pub part_type: String,
    pub text: Option<String>,
    pub text_html: Option<String>,
    pub tool_name: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    pub tool_output: Option<String>,
    pub tool_status: Option<String>,
    pub tool_title: Option<String>,
}

/// Default hstry database path, if it exists.
pub fn hstry_db_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("OQTO_HSTRY_DB") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    if let Some(path) = hstry_db_path_from_config()
        && path.exists()
    {
        return Some(path);
    }

    let default = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("hstry")
        .join("hstry.db");
    if default.exists() {
        Some(default)
    } else {
        None
    }
}

fn hstry_db_path_from_config() -> Option<PathBuf> {
    let config_dir = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))?;
    let config_path = config_dir.join("hstry").join("config.toml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let parsed: toml::Value = content.parse().ok()?;
    let db_str = parsed.get("database")?.as_str()?;
    Some(PathBuf::from(db_str))
}

pub fn project_name_from_path(path: &str) -> String {
    if path == "global" || path.is_empty() {
        return "Global".to_string();
    }
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

pub async fn open_hstry_pool(db_path: &Path) -> Result<sqlx::SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .read_only(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await?;
    Ok(pool)
}

pub async fn get_session_via_grpc(
    client: &HstryClient,
    session_id: &str,
) -> Result<Option<RunnerChatSession>> {
    let conv = client.get_conversation(session_id, None).await?;
    Ok(conv.map(|c| RunnerChatSession {
        workspace_path: c.workspace.unwrap_or_default(),
    }))
}

pub async fn get_session_messages_from_hstry(
    session_id: &str,
    db_path: &Path,
) -> Result<Vec<ChatMessage>> {
    let pool = open_hstry_pool(db_path).await?;
    let conversation_row = sqlx::query(
        r#"
        SELECT c.id
        FROM conversations c
        LEFT JOIN messages m ON m.conversation_id = c.id
        WHERE c.external_id = ? OR c.platform_id = ? OR c.readable_id = ? OR c.id = ?
        GROUP BY c.id
        ORDER BY COUNT(m.id) DESC, COALESCE(c.updated_at, c.created_at) DESC
        LIMIT 1
        "#,
    )
    .bind(session_id)
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
        SELECT id, role, content, created_at, model, tokens, cost_usd, parts_json, client_id
        FROM messages
        WHERE conversation_id = ?
        ORDER BY idx
        "#,
    )
    .bind(conversation_id)
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
        let client_id: Option<String> = row.get("client_id");

        let mut parts = hstry_parts_to_chat_parts(parts_json.as_deref(), &content, &id);

        if role == "tool" || role == "toolResult" {
            let has_tool_result = parts.iter().any(|p| p.part_type == "tool_result");
            if has_tool_result {
                parts.retain(|p| p.part_type != "text");
            }
        }

        messages.push(ChatMessage {
            id,
            session_id: session_id.to_string(),
            role,
            created_at: created_at.map(|ts| ts * 1000).unwrap_or(0),
            completed_at: None,
            parent_id: None,
            model_id: model,
            provider_id: None,
            agent: None,
            summary_title: None,
            tokens_input: None,
            tokens_output: tokens,
            tokens_reasoning: None,
            cost,
            client_id,
            parts,
        });
    }

    Ok(messages)
}

fn hstry_parts_to_chat_parts(
    parts_json: Option<&str>,
    content: &str,
    message_id: &str,
) -> Vec<ChatMessagePart> {
    if let Some(parts_json) = parts_json
        && let Ok(serde_json::Value::Array(values)) = serde_json::from_str(parts_json)
    {
        let mut parts = Vec::new();
        for (idx, value) in values.iter().enumerate() {
            let serde_json::Value::Object(obj) = value else {
                continue;
            };
            let part_type_raw = obj.get("type").and_then(|v| v.as_str()).unwrap_or("text");
            let normalized_type = part_type_raw.to_lowercase();
            let id = format!("{message_id}-part-{idx}");

            let text = match normalized_type.as_str() {
                "text" | "thinking" => obj.get("text").and_then(|v| v.as_str()),
                _ => None,
            }
            .map(ToOwned::to_owned);

            let tool_name = obj
                .get("name")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned);

            let tool_call_id = obj
                .get("toolCallId")
                .or_else(|| obj.get("tool_call_id"))
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned);

            let tool_input = obj.get("input").cloned();

            let tool_output = obj
                .get("output")
                .and_then(|v| {
                    if v.is_string() {
                        v.as_str().map(ToOwned::to_owned)
                    } else {
                        Some(v.to_string())
                    }
                })
                .or_else(|| {
                    obj.get("text")
                        .and_then(|v| v.as_str())
                        .map(ToOwned::to_owned)
                });

            let tool_status = obj
                .get("status")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned);
            let tool_title = obj
                .get("title")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned);

            let part_type = match normalized_type.as_str() {
                "tool_call" => "tool_call",
                "tool_result" => "tool_result",
                "thinking" => "thinking",
                _ => "text",
            }
            .to_string();

            parts.push(ChatMessagePart {
                id,
                part_type,
                text,
                text_html: None,
                tool_name,
                tool_call_id,
                tool_input,
                tool_output,
                tool_status,
                tool_title,
            });
        }

        if !parts.is_empty() {
            return parts;
        }
    }

    vec![ChatMessagePart {
        id: format!("{message_id}-part-0"),
        part_type: "text".to_string(),
        text: Some(content.to_string()),
        text_html: None,
        tool_name: None,
        tool_call_id: None,
        tool_input: None,
        tool_output: None,
        tool_status: None,
        tool_title: None,
    }]
}
