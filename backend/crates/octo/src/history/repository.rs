//! Repository layer for chat history - handles file and database operations.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use serde_json;
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tokio::sync::Mutex;

use crate::{wordlist, workspace};

use super::models::{
    ChatMessage, ChatMessagePart, ChatSession, ChatSessionStats, MessageInfo, PartInfo, SessionInfo,
};

/// Default OpenCode data directory.
pub fn default_opencode_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opencode")
}

/// Default hstry database path, if it exists.
pub fn hstry_db_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("OCTO_HSTRY_DB") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
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

/// Extract project name from workspace path.
pub fn project_name_from_path(path: &str) -> String {
    if path == "global" || path.is_empty() {
        return "Global".to_string();
    }
    let path_buf = Path::new(path);
    if path_buf.is_dir()
        && let Some(display_name) = workspace::workspace_display_name(path_buf)
    {
        return display_name;
    }
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

// ============================================================================
// SQLite/hstry repository functions
// ============================================================================

static HSTRY_POOL_CACHE: Lazy<Mutex<HashMap<PathBuf, sqlx::SqlitePool>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub async fn open_hstry_pool(db_path: &Path) -> Result<sqlx::SqlitePool> {
    let db_path = db_path.to_path_buf();

    {
        let cache = HSTRY_POOL_CACHE.lock().await;
        if let Some(pool) = cache.get(&db_path) {
            return Ok(pool.clone());
        }
    }

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .read_only(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await?;

    let mut cache = HSTRY_POOL_CACHE.lock().await;
    cache.insert(db_path, pool.clone());
    Ok(pool)
}

pub fn hstry_timestamp_ms(value: Option<i64>) -> Option<i64> {
    value.map(|ts| ts * 1000)
}

pub async fn list_sessions_from_hstry(db_path: &Path) -> Result<Vec<ChatSession>> {
    let pool = open_hstry_pool(db_path).await?;
    let rows = sqlx::query(
        r#"
        SELECT id, external_id, readable_id, title, created_at, updated_at, workspace, model, provider
        FROM conversations
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(&pool)
    .await?;

    let mut sessions = Vec::new();
    for row in rows {
        let id: String = row.get("id");
        let external_id: Option<String> = row.get("external_id");
        let readable_id: Option<String> = row.get("readable_id");
        let title: Option<String> = row.get("title");
        let created_at: i64 = row.get("created_at");
        let updated_at: Option<i64> = row.get("updated_at");
        let workspace: Option<String> = row.get("workspace");
        let model: Option<String> = row.get("model");
        let provider: Option<String> = row.get("provider");

        let session_id = external_id.clone().unwrap_or_else(|| id.clone());
        let workspace_path = workspace.unwrap_or_else(|| "global".to_string());
        let project_name = project_name_from_path(&workspace_path);
        let readable_id = readable_id.unwrap_or_default();

        sessions.push(ChatSession {
            id: session_id,
            readable_id,
            title,
            parent_id: None,
            workspace_path,
            project_name,
            created_at: created_at * 1000,
            updated_at: updated_at.map(|ts| ts * 1000).unwrap_or(created_at * 1000),
            version: None,
            is_child: false,
            source_path: None,
            stats: None,
            model,
            provider,
        });
    }

    Ok(sessions)
}

pub async fn get_session_from_hstry(
    session_id: &str,
    db_path: &Path,
) -> Result<Option<ChatSession>> {
    let pool = open_hstry_pool(db_path).await?;
    let row = sqlx::query(
        r#"
        SELECT id, external_id, readable_id, title, created_at, updated_at, workspace, model, provider
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

    let Some(row) = row else {
        return Ok(None);
    };

    let id: String = row.get("id");
    let external_id: Option<String> = row.get("external_id");
    let readable_id: Option<String> = row.get("readable_id");
    let title: Option<String> = row.get("title");
    let created_at: i64 = row.get("created_at");
    let updated_at: Option<i64> = row.get("updated_at");
    let workspace: Option<String> = row.get("workspace");
    let model: Option<String> = row.get("model");
    let provider: Option<String> = row.get("provider");

    let session_id = external_id.clone().unwrap_or_else(|| id.clone());
    let workspace_path = workspace.unwrap_or_else(|| "global".to_string());
    let project_name = project_name_from_path(&workspace_path);
    let readable_id = readable_id.unwrap_or_default();

    Ok(Some(ChatSession {
        id: session_id,
        readable_id,
        title,
        parent_id: None,
        workspace_path,
        project_name,
        created_at: created_at * 1000,
        updated_at: updated_at.map(|ts| ts * 1000).unwrap_or(created_at * 1000),
        version: None,
        is_child: false,
        source_path: None,
        stats: None,
        model,
        provider,
    }))
}

pub async fn get_session_messages_from_hstry(
    session_id: &str,
    db_path: &Path,
) -> Result<Vec<ChatMessage>> {
    let pool = open_hstry_pool(db_path).await?;
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

        // For tool result messages, strip text parts that duplicate the tool output.
        // hstry may store both a text part and a tool_result part for the same content.
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
            created_at: hstry_timestamp_ms(created_at).unwrap_or(0),
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
    let mut parts = Vec::new();

    if let Some(parts_json) = parts_json {
        if let Ok(canon_parts) = serde_json::from_str::<Vec<crate::canon::CanonPart>>(parts_json) {
            for (idx, part) in canon_parts.into_iter().enumerate() {
                let id = format!("{message_id}-part-{idx}");
                match part {
                    crate::canon::CanonPart::Text { text, .. } => parts.push(ChatMessagePart {
                        id,
                        part_type: "text".to_string(),
                        text: Some(text),
                        text_html: None,
                        tool_name: None,
                        tool_input: None,
                        tool_output: None,
                        tool_status: None,
                        tool_title: None,
                    }),
                    crate::canon::CanonPart::Thinking { text, .. } => parts.push(ChatMessagePart {
                        id,
                        part_type: "thinking".to_string(),
                        text: Some(text),
                        text_html: None,
                        tool_name: None,
                        tool_input: None,
                        tool_output: None,
                        tool_status: None,
                        tool_title: None,
                    }),
                    crate::canon::CanonPart::ToolCall {
                        name,
                        input,
                        status,
                        ..
                    } => parts.push(ChatMessagePart {
                        id,
                        part_type: "tool_call".to_string(),
                        text: None,
                        text_html: None,
                        tool_name: Some(name),
                        tool_input: input,
                        tool_output: None,
                        tool_status: Some(match status {
                            crate::canon::ToolStatus::Pending => "pending".to_string(),
                            crate::canon::ToolStatus::Running => "running".to_string(),
                            crate::canon::ToolStatus::Success => "success".to_string(),
                            crate::canon::ToolStatus::Error => "error".to_string(),
                        }),
                        tool_title: None,
                    }),
                    crate::canon::CanonPart::ToolResult {
                        name,
                        output,
                        is_error,
                        title,
                        ..
                    } => parts.push(ChatMessagePart {
                        id,
                        part_type: "tool_result".to_string(),
                        text: None,
                        text_html: None,
                        tool_name: name,
                        tool_input: None,
                        tool_output: output.as_ref().map(|v| v.to_string()),
                        tool_status: Some(if is_error { "error" } else { "success" }.to_string()),
                        tool_title: title,
                    }),
                    _ => {}
                }
            }
            return parts;
        }

        if let Ok(serde_json::Value::Array(values)) = serde_json::from_str(parts_json) {
            for (idx, value) in values.iter().enumerate() {
                let serde_json::Value::Object(obj) = value else {
                    continue;
                };
                let part_type = obj
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("text")
                    .to_string();
                let text = match part_type.as_str() {
                    "text" | "thinking" => obj.get("text").and_then(|v| v.as_str()),
                    "status" | "error" => obj
                        .get("message")
                        .or_else(|| obj.get("text"))
                        .and_then(|v| v.as_str()),
                    _ => None,
                };
                if let Some(text) = text {
                    parts.push(ChatMessagePart {
                        id: format!("{message_id}-part-{idx}"),
                        part_type,
                        text: Some(text.to_string()),
                        text_html: None,
                        tool_name: None,
                        tool_input: None,
                        tool_output: None,
                        tool_status: None,
                        tool_title: None,
                    });
                }
            }
            return parts;
        }
    }

    if parts.is_empty() && !content.trim().is_empty() {
        parts.push(ChatMessagePart {
            id: format!("{message_id}-part-0"),
            part_type: "text".to_string(),
            text: Some(content.to_string()),
            text_html: None,
            tool_name: None,
            tool_input: None,
            tool_output: None,
            tool_status: None,
            tool_title: None,
        });
    }

    parts
}

// ============================================================================
// gRPC-based repository functions (via HstryClient)
// ============================================================================

/// List all sessions from hstry via gRPC.
pub async fn list_sessions_via_grpc(
    client: &crate::hstry::HstryClient,
) -> Result<Vec<ChatSession>> {
    let summaries = client.list_conversations(None, None, None).await?;
    let sessions = summaries
        .into_iter()
        .filter_map(|s| s.conversation.map(|c| conversation_proto_to_session(&c)))
        .collect();
    Ok(sessions)
}

/// Get a single session from hstry via gRPC.
pub async fn get_session_via_grpc(
    client: &crate::hstry::HstryClient,
    session_id: &str,
) -> Result<Option<ChatSession>> {
    let conv = client.get_conversation(session_id, None).await?;
    Ok(conv.map(|c| conversation_proto_to_session(&c)))
}

/// Get messages for a session from hstry via gRPC.
pub async fn get_session_messages_via_grpc(
    client: &crate::hstry::HstryClient,
    session_id: &str,
) -> Result<Vec<ChatMessage>> {
    let proto_messages = client.get_messages(session_id, None, None).await?;
    let messages = proto_messages
        .iter()
        .map(|m| message_proto_to_chat_message(m, session_id))
        .collect();
    Ok(messages)
}

/// Convert a proto Conversation to a ChatSession.
fn conversation_proto_to_session(conv: &hstry_core::service::proto::Conversation) -> ChatSession {
    let stats = parse_stats_from_metadata(&conv.metadata_json);
    let workspace_path = conv
        .workspace
        .clone()
        .unwrap_or_else(|| "global".to_string());
    let project_name = project_name_from_path(&workspace_path);

    ChatSession {
        id: conv.external_id.clone(),
        readable_id: String::new(),
        title: conv.title.clone(),
        parent_id: None,
        workspace_path,
        project_name,
        created_at: conv.created_at_ms,
        updated_at: conv.updated_at_ms.unwrap_or(conv.created_at_ms),
        version: None,
        is_child: false,
        source_path: None,
        stats,
        model: conv.model.clone(),
        provider: conv.provider.clone(),
    }
}

fn parse_stats_from_metadata(metadata_json: &str) -> Option<ChatSessionStats> {
    if metadata_json.trim().is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(metadata_json).ok()?;
    let stats = value.get("stats")?.as_object()?;
    Some(ChatSessionStats {
        tokens_in: stats.get("tokens_in").and_then(|v| v.as_i64()).unwrap_or(0),
        tokens_out: stats
            .get("tokens_out")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        cache_read: stats
            .get("cache_read")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        cache_write: stats
            .get("cache_write")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        cost_usd: stats
            .get("cost_usd")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
    })
}

/// Convert a proto Message to a ChatMessage.
fn message_proto_to_chat_message(
    msg: &hstry_core::service::proto::Message,
    session_id: &str,
) -> ChatMessage {
    let id = format!("{session_id}-msg-{}", msg.idx);
    let mut parts = hstry_parts_to_chat_parts(
        if msg.parts_json.is_empty() {
            None
        } else {
            Some(msg.parts_json.as_str())
        },
        &msg.content,
        &id,
    );

    // For tool result messages, strip text parts that duplicate the tool output.
    // hstry may store both a text part and a tool_result part for the same content.
    if msg.role == "tool" || msg.role == "toolResult" {
        let has_tool_result = parts.iter().any(|p| p.part_type == "tool_result");
        if has_tool_result {
            parts.retain(|p| p.part_type != "text");
        }
    }

    ChatMessage {
        id,
        session_id: session_id.to_string(),
        role: msg.role.clone(),
        created_at: msg.created_at_ms.unwrap_or(0),
        completed_at: None,
        parent_id: None,
        model_id: msg.model.clone(),
        provider_id: msg.provider.clone(),
        agent: None,
        summary_title: None,
        tokens_input: None,
        tokens_output: msg.tokens,
        tokens_reasoning: None,
        cost: msg.cost_usd,
        client_id: msg.client_id.clone(),
        parts,
    }
}

// ============================================================================
// File-based repository functions
// ============================================================================

/// Read all chat sessions from OpenCode's data directory.
pub fn list_sessions() -> Result<Vec<ChatSession>> {
    list_sessions_from_dir(&default_opencode_data_dir())
}

/// Read all chat sessions from a specific OpenCode data directory.
///
/// OpenCode stores sessions in: {opencode_dir}/storage/session/{projectID}/ses_*.json
pub fn list_sessions_from_dir(opencode_dir: &Path) -> Result<Vec<ChatSession>> {
    let session_dir = opencode_dir.join("storage/session");

    if !session_dir.exists() {
        tracing::debug!("Session directory does not exist: {:?}", session_dir);
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();

    // Iterate over project hash directories
    let project_entries = std::fs::read_dir(&session_dir)
        .with_context(|| format!("reading session dir: {:?}", session_dir))?;

    for project_entry in project_entries {
        let project_entry = match project_entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }

        // Read session files in this project directory
        let session_entries = match std::fs::read_dir(&project_path) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for session_entry in session_entries {
            let session_entry = match session_entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let session_path = session_entry.path();

            // Only process ses_*.json files
            let is_session_file = session_path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|name| name.starts_with("ses_") && name.ends_with(".json"))
                .unwrap_or(false);

            if !is_session_file {
                continue;
            }

            // Skip if not a regular file
            if !session_path.is_file() {
                continue;
            }

            // Read and parse session info
            let content = match std::fs::read_to_string(&session_path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::debug!("Failed to read session file {:?}: {}", session_path, e);
                    continue;
                }
            };

            let info: SessionInfo = match serde_json::from_str(&content) {
                Ok(i) => i,
                Err(e) => {
                    tracing::debug!("Failed to parse session file {:?}: {}", session_path, e);
                    continue;
                }
            };

            // Get workspace path from the session's directory field
            let workspace_path = info
                .directory
                .clone()
                .unwrap_or_else(|| "global".to_string());
            let project_name = project_name_from_path(&workspace_path);
            let is_child = info.parent_id.is_some();

            sessions.push(ChatSession {
                id: info.id.clone(),
                readable_id: wordlist::readable_id_from_session_id(&info.id),
                title: info.title,
                parent_id: info.parent_id,
                workspace_path,
                project_name,
                created_at: info.time.created,
                updated_at: info.time.updated,
                version: info.version,
                is_child,
                source_path: Some(session_path.to_string_lossy().to_string()),
                stats: None,
                model: None,
                provider: None,
            });
        }
    }

    // Sort by updated_at descending (most recent first)
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    tracing::info!("Found {} sessions in {:?}", sessions.len(), session_dir);

    Ok(sessions)
}

/// List sessions grouped by project/workspace.
pub fn list_sessions_grouped() -> Result<HashMap<String, Vec<ChatSession>>> {
    let sessions = list_sessions()?;
    let mut grouped: HashMap<String, Vec<ChatSession>> = HashMap::new();

    for session in sessions {
        grouped
            .entry(session.workspace_path.clone())
            .or_default()
            .push(session);
    }

    Ok(grouped)
}

/// Get a single session by ID.
pub fn get_session(session_id: &str) -> Result<Option<ChatSession>> {
    get_session_from_dir(session_id, &default_opencode_data_dir())
}

/// Get a single session by ID from a specific OpenCode data directory.
pub fn get_session_from_dir(session_id: &str, opencode_dir: &Path) -> Result<Option<ChatSession>> {
    let sessions = list_sessions_from_dir(opencode_dir)?;
    Ok(sessions.into_iter().find(|s| s.id == session_id))
}

/// Update a session's title on disk.
///
/// This reads the session JSON file, updates the title field, and writes it back.
/// Returns the updated session or an error if the session doesn't exist.
pub fn update_session_title(session_id: &str, new_title: &str) -> Result<ChatSession> {
    update_session_title_in_dir(session_id, new_title, &default_opencode_data_dir())
}

/// Update a session's title on disk from a specific OpenCode data directory.
pub fn update_session_title_in_dir(
    session_id: &str,
    new_title: &str,
    opencode_dir: &Path,
) -> Result<ChatSession> {
    let session_dir = opencode_dir.join("storage/session");

    if !session_dir.exists() {
        anyhow::bail!("Session directory does not exist");
    }

    // Find the session file by iterating through project directories
    let project_entries = std::fs::read_dir(&session_dir)
        .with_context(|| format!("reading session dir: {:?}", session_dir))?;

    for project_entry in project_entries {
        let project_entry = match project_entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }

        // Look for the session file in this project directory
        let session_file = project_path.join(format!("{}.json", session_id));
        if !session_file.exists() {
            continue;
        }

        // Found the session file - read, update, and write back
        let content = std::fs::read_to_string(&session_file)
            .with_context(|| format!("reading session file: {:?}", session_file))?;

        let mut info: SessionInfo = serde_json::from_str(&content)
            .with_context(|| format!("parsing session file: {:?}", session_file))?;

        // Update the title and updated timestamp
        info.title = Some(new_title.to_string());
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(info.time.updated);
        info.time.updated = now_ms;

        // Write back
        let updated_content =
            serde_json::to_string_pretty(&info).with_context(|| "serializing updated session")?;
        std::fs::write(&session_file, updated_content)
            .with_context(|| format!("writing session file: {:?}", session_file))?;

        // Return the updated session
        let workspace_path = info
            .directory
            .clone()
            .unwrap_or_else(|| "global".to_string());
        let project_name = project_name_from_path(&workspace_path);
        let is_child = info.parent_id.is_some();

        tracing::info!("Updated session {} title to: {}", session_id, new_title);

        return Ok(ChatSession {
            id: info.id.clone(),
            readable_id: wordlist::readable_id_from_session_id(&info.id),
            title: info.title,
            parent_id: info.parent_id,
            workspace_path,
            project_name,
            created_at: info.time.created,
            updated_at: info.time.updated,
            version: info.version,
            is_child,
            source_path: Some(session_file.to_string_lossy().to_string()),
            stats: None,
            model: None,
            provider: None,
        });
    }

    anyhow::bail!("Session not found: {}", session_id)
}

/// Get all messages for a session using parallel I/O.
pub async fn get_session_messages_parallel(
    session_id: &str,
    opencode_dir: &Path,
) -> Result<Vec<ChatMessage>> {
    let message_dir = opencode_dir.join("storage/message").join(session_id);
    let part_dir = opencode_dir.join("storage/part");

    if !message_dir.exists() {
        tracing::debug!("Message directory does not exist: {:?}", message_dir);
        return Ok(Vec::new());
    }

    // Read message directory entries
    let message_entries: Vec<_> = std::fs::read_dir(&message_dir)
        .with_context(|| format!("reading message dir: {:?}", message_dir))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .file_name()
                .and_then(|s| s.to_str())
                .map(|name| name.starts_with("msg_") && name.ends_with(".json"))
                .unwrap_or(false)
        })
        .collect();

    // Spawn tasks to read messages in parallel
    let mut tasks = Vec::with_capacity(message_entries.len());

    for entry in message_entries {
        let msg_path = entry.path();
        let part_dir = part_dir.clone();

        tasks.push(tokio::task::spawn_blocking(move || {
            load_single_message(&msg_path, &part_dir)
        }));
    }

    // Wait for all tasks and collect results
    let mut messages = Vec::new();
    for task in tasks {
        if let Ok(Ok(Some(msg))) = task.await {
            messages.push(msg);
        }
    }

    // Sort by created_at ascending (chronological order)
    messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    tracing::debug!(
        "Loaded {} messages for session {} using parallel I/O",
        messages.len(),
        session_id
    );

    Ok(messages)
}

/// Load a single message and its parts.
fn load_single_message(msg_path: &Path, part_dir: &Path) -> Result<Option<ChatMessage>> {
    if !msg_path.is_file() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(msg_path)
        .with_context(|| format!("reading message: {:?}", msg_path))?;

    let info: MessageInfo = serde_json::from_str(&content)
        .with_context(|| format!("parsing message: {:?}", msg_path))?;

    // Load parts for this message
    let parts = load_message_parts(&info.id, &info.session_id, part_dir);

    Ok(Some(ChatMessage {
        id: info.id.clone(),
        session_id: info.session_id,
        role: info.role,
        created_at: info.time.created,
        completed_at: info.time.completed,
        parent_id: info.parent_id,
        model_id: info.model_id,
        provider_id: info.provider_id,
        agent: info.agent,
        summary_title: info.summary.and_then(|s| s.title),
        tokens_input: info.tokens.as_ref().and_then(|t| t.input),
        tokens_output: info.tokens.as_ref().and_then(|t| t.output),
        tokens_reasoning: info.tokens.as_ref().and_then(|t| t.reasoning),
        cost: info.cost,
        client_id: None,
        parts,
    }))
}

/// Get all messages for a session from a specific OpenCode data directory.
pub fn get_session_messages_from_dir(
    session_id: &str,
    opencode_dir: &Path,
) -> Result<Vec<ChatMessage>> {
    let message_dir = opencode_dir.join("storage/message").join(session_id);
    let part_dir = opencode_dir.join("storage/part");

    if !message_dir.exists() {
        tracing::debug!("Message directory does not exist: {:?}", message_dir);
        return Ok(Vec::new());
    }

    let mut messages = Vec::new();

    // Read all message files for this session
    let message_entries = std::fs::read_dir(&message_dir)
        .with_context(|| format!("reading message dir: {:?}", message_dir))?;

    for entry in message_entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let msg_path = entry.path();

        // Only process msg_*.json files
        let is_message_file = msg_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|name| name.starts_with("msg_") && name.ends_with(".json"))
            .unwrap_or(false);

        if !is_message_file || !msg_path.is_file() {
            continue;
        }

        // Read and parse message info
        let content = match std::fs::read_to_string(&msg_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!("Failed to read message file {:?}: {}", msg_path, e);
                continue;
            }
        };

        let info: MessageInfo = match serde_json::from_str(&content) {
            Ok(i) => i,
            Err(e) => {
                tracing::debug!("Failed to parse message file {:?}: {}", msg_path, e);
                continue;
            }
        };

        // Load parts for this message
        let parts = load_message_parts(&info.id, &info.session_id, &part_dir);

        messages.push(ChatMessage {
            id: info.id.clone(),
            session_id: info.session_id,
            role: info.role,
            created_at: info.time.created,
            completed_at: info.time.completed,
            parent_id: info.parent_id,
            model_id: info.model_id,
            provider_id: info.provider_id,
            agent: info.agent,
            summary_title: info.summary.and_then(|s| s.title),
            tokens_input: info.tokens.as_ref().and_then(|t| t.input),
            tokens_output: info.tokens.as_ref().and_then(|t| t.output),
            tokens_reasoning: info.tokens.as_ref().and_then(|t| t.reasoning),
            cost: info.cost,
            client_id: None,
            parts,
        });
    }

    // Sort by created_at ascending (chronological order)
    messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    tracing::debug!(
        "Loaded {} messages for session {} from {:?}",
        messages.len(),
        session_id,
        message_dir
    );

    Ok(messages)
}

/// Load all parts for a specific message.
fn load_message_parts(message_id: &str, session_id: &str, part_dir: &Path) -> Vec<ChatMessagePart> {
    let msg_part_dir = part_dir.join(message_id);

    if !msg_part_dir.exists() {
        return Vec::new();
    }

    let mut parts = Vec::new();

    let entries = match std::fs::read_dir(&msg_part_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let part_path = entry.path();

        // Only process prt_*.json files
        let is_part_file = part_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|name| name.starts_with("prt_") && name.ends_with(".json"))
            .unwrap_or(false);

        if !is_part_file || !part_path.is_file() {
            continue;
        }

        let content = match std::fs::read_to_string(&part_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let info: PartInfo = match serde_json::from_str(&content) {
            Ok(i) => i,
            Err(_) => continue,
        };
        if info.message_id != message_id || info.session_id != session_id {
            tracing::debug!(
                "Skipping part {} for mismatched IDs (message={}, session={})",
                info.id,
                info.message_id,
                info.session_id
            );
            continue;
        }

        // Convert to ChatMessagePart based on type
        let part = match info.part_type.as_str() {
            "text" => ChatMessagePart {
                id: info.id,
                part_type: info.part_type,
                text: info.text,
                text_html: None, // Rendered on-demand via separate endpoint
                tool_name: None,
                tool_input: None,
                tool_output: None,
                tool_status: None,
                tool_title: None,
            },
            "tool" => ChatMessagePart {
                id: info.id,
                part_type: info.part_type,
                text: None,
                text_html: None,
                tool_name: info.tool,
                tool_input: info.state.as_ref().and_then(|s| s.input.clone()),
                tool_output: info.state.as_ref().and_then(|s| s.output.clone()),
                tool_status: info.state.as_ref().and_then(|s| s.status.clone()),
                tool_title: info.state.as_ref().and_then(|s| s.title.clone()),
            },
            // For step-start, step-finish, and other types, include minimal info
            _ => ChatMessagePart {
                id: info.id,
                part_type: info.part_type,
                text: info.text,
                text_html: None,
                tool_name: None,
                tool_input: None,
                tool_output: None,
                tool_status: None,
                tool_title: None,
            },
        };

        parts.push(part);
    }

    // Sort parts by ID (which should be roughly chronological)
    parts.sort_by(|a, b| a.id.cmp(&b.id));

    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_name_from_path() {
        assert_eq!(project_name_from_path("global"), "Global");
        assert_eq!(project_name_from_path(""), "Global");
        assert_eq!(project_name_from_path("/home/wismut/Code/lst"), "lst");
        assert_eq!(
            project_name_from_path("/home/wismut/byteowlz/kittenx"),
            "kittenx"
        );
        assert_eq!(
            project_name_from_path("/home/wismut/byteowlz/govnr"),
            "govnr"
        );
    }
}
