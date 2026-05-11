//! Read-only oqto-log projector owned by the history crate.
//!
//! This module intentionally returns neutral `oqto_protocol::projection` DTOs so
//! storage/projection code does not depend on runner wire types.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use oqto_protocol::events::MessageVersion;
use oqto_protocol::projection::{
    ProjectedChatMessage, ProjectedChatMessagePart, ProjectedTurnTreeNode,
};
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tokio::sync::Mutex;

use crate::oqto_log::paths::resolve_user_home_workspace_db_path;

static PROJECTOR_POOLS: Lazy<Mutex<HashMap<PathBuf, sqlx::SqlitePool>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

async fn open_pool_for_workspace(user_home: &Path, workspace_id: &str) -> Result<sqlx::SqlitePool> {
    let db_path = resolve_user_home_workspace_db_path(user_home, workspace_id)?;

    {
        let pools = PROJECTOR_POOLS.lock().await;
        if let Some(pool) = pools.get(&db_path) {
            return Ok(pool.clone());
        }
    }

    if !db_path.exists() {
        anyhow::bail!("oqto-log db does not exist");
    }

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .read_only(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .with_context(|| format!("opening oqto-log db: {}", db_path.display()))?;

    let mut pools = PROJECTOR_POOLS.lock().await;
    pools.insert(db_path, pool.clone());
    Ok(pool)
}

fn extract_client_id_from_payload_json(payload: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    let obj = value.as_object()?;

    if let Some(extra_obj) = obj.get("extra").and_then(|v| v.as_object()) {
        for key in ["client_id", "clientId", "oqto_client_id"] {
            if let Some(client_id) = extra_obj.get(key).and_then(|v| v.as_str()) {
                let trimmed = client_id.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    obj.get("client_id")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("clientId").and_then(|v| v.as_str()))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

async fn list_workspace_hash_dirs(user_home: &Path) -> Vec<PathBuf> {
    let root = user_home
        .join(".local")
        .join("share")
        .join("oqto")
        .join("oqto-log");
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.push(path);
        }
    }
    out
}

pub async fn project_session_messages_auto(
    user_home: &Path,
    session_id: &str,
    limit: Option<usize>,
) -> Result<Option<Vec<ProjectedChatMessage>>> {
    let dirs = list_workspace_hash_dirs(user_home).await;
    for dir in dirs {
        let db_path = dir.join("oqto-log.sqlite");
        if !db_path.exists() {
            continue;
        }

        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .read_only(true);
        let pool = match SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
        {
            Ok(pool) => pool,
            Err(_) => continue,
        };

        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM oqto_log_sessions WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

        if exists <= 0 {
            continue;
        }

        let mut rows = sqlx::query(
            r#"
            SELECT
              m.message_id AS message_id,
              t.parent_turn_id AS parent_turn_id,
              t.role AS role,
              m.content AS content,
              m.json_payload AS json_payload,
              CAST(strftime('%s', COALESCE(t.committed_at, t.created_at)) * 1000 AS INTEGER) AS created_at_ms
            FROM oqto_log_turns t
            JOIN oqto_log_messages m ON m.turn_id = t.turn_id
            WHERE t.session_id = ?
            ORDER BY t.turn_version ASC, m.seq ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        if let Some(l) = limit
            && rows.len() > l
        {
            rows = rows.split_off(rows.len() - l);
        }

        let mapped = rows
            .into_iter()
            .enumerate()
            .map(|(idx, row)| row_to_projected_message(idx, session_id, row))
            .collect();

        return Ok(Some(mapped));
    }

    Ok(None)
}

pub async fn project_session_tree_auto(
    user_home: &Path,
    session_id: &str,
) -> Result<Option<Vec<ProjectedTurnTreeNode>>> {
    let dirs = list_workspace_hash_dirs(user_home).await;
    for dir in dirs {
        let db_path = dir.join("oqto-log.sqlite");
        if !db_path.exists() {
            continue;
        }

        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .read_only(true);
        let pool = match SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
        {
            Ok(pool) => pool,
            Err(_) => continue,
        };

        let rows = sqlx::query(
            r#"
            SELECT turn_id, parent_turn_id, branch_id, role, turn_version
            FROM oqto_log_turns
            WHERE session_id = ?
            ORDER BY turn_version ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        if rows.is_empty() {
            continue;
        }

        let tree = rows
            .into_iter()
            .map(|row| ProjectedTurnTreeNode {
                turn_id: row.try_get::<String, _>("turn_id").unwrap_or_default(),
                parent_turn_id: row
                    .try_get::<Option<String>, _>("parent_turn_id")
                    .ok()
                    .flatten(),
                branch_id: row.try_get::<String, _>("branch_id").unwrap_or_default(),
                role: row.try_get::<String, _>("role").unwrap_or_default(),
                turn_version: row.try_get::<i64, _>("turn_version").unwrap_or_default(),
            })
            .collect();

        return Ok(Some(tree));
    }

    Ok(None)
}

pub async fn read_message_version_auto(
    user_home: &Path,
    session_id: &str,
) -> Result<Option<MessageVersion>> {
    let dirs = list_workspace_hash_dirs(user_home).await;
    for dir in dirs {
        let db_path = dir.join("oqto-log.sqlite");
        if !db_path.exists() {
            continue;
        }

        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .read_only(true);
        let pool = match SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
        {
            Ok(pool) => pool,
            Err(_) => continue,
        };

        let row = sqlx::query(
            r#"
            SELECT
              COALESCE(MAX(turn_version), 0) AS version,
              (SELECT COUNT(*) FROM oqto_log_messages m
                 JOIN oqto_log_turns t ON t.turn_id = m.turn_id
                WHERE t.session_id = ?) AS message_count
            FROM oqto_log_turns
            WHERE session_id = ?
            "#,
        )
        .bind(session_id)
        .bind(session_id)
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten();

        let Some(row) = row else {
            continue;
        };

        let version: i64 = row.try_get("version").unwrap_or(0);
        let message_count: i64 = row.try_get("message_count").unwrap_or(0);

        if version > 0 || message_count > 0 {
            return Ok(Some(MessageVersion {
                version: version.max(0) as u64,
                message_count: Some(message_count.max(0) as u64),
                last_message_hash: None,
            }));
        }
    }

    Ok(None)
}

#[allow(dead_code)]
pub async fn project_session_messages_for_workspace(
    user_home: &Path,
    workspace_id: &str,
    session_id: &str,
    limit: Option<usize>,
) -> Result<Vec<ProjectedChatMessage>> {
    let pool = open_pool_for_workspace(user_home, workspace_id).await?;
    let mut rows = sqlx::query(
        r#"
        SELECT
          m.message_id AS message_id,
          t.parent_turn_id AS parent_turn_id,
          t.role AS role,
          m.content AS content,
          m.json_payload AS json_payload,
          CAST(strftime('%s', COALESCE(t.committed_at, t.created_at)) * 1000 AS INTEGER) AS created_at_ms
        FROM oqto_log_turns t
        JOIN oqto_log_messages m ON m.turn_id = t.turn_id
        WHERE t.session_id = ?
        ORDER BY t.turn_version ASC, m.seq ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(&pool)
    .await
    .context("query oqto-log projection")?;

    if let Some(l) = limit
        && rows.len() > l
    {
        rows = rows.split_off(rows.len() - l);
    }

    Ok(rows
        .into_iter()
        .enumerate()
        .map(|(idx, row)| row_to_projected_message(idx, session_id, row))
        .collect())
}

fn projected_parts_from_payload(
    msg_id: &str,
    fallback_content: Option<String>,
    json_payload: Option<&str>,
) -> Vec<ProjectedChatMessagePart> {
    let Some(payload) = json_payload else {
        return fallback_text_part(msg_id, fallback_content);
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
        return fallback_text_part(msg_id, fallback_content);
    };
    let Some(content) = value.get("content") else {
        return fallback_text_part(msg_id, fallback_content);
    };
    let mut parts = Vec::new();
    match content {
        serde_json::Value::Array(items) => {
            for (idx, item) in items.iter().enumerate() {
                if let Some(part) = content_item_to_projected_part(msg_id, idx, item) {
                    parts.push(part);
                }
            }
        }
        _ => return fallback_text_part(msg_id, fallback_content),
    }
    if parts.is_empty() {
        fallback_text_part(msg_id, fallback_content)
    } else {
        parts
    }
}

fn fallback_text_part(msg_id: &str, text: Option<String>) -> Vec<ProjectedChatMessagePart> {
    vec![ProjectedChatMessagePart {
        id: format!("{}:part:0", msg_id),
        part_type: "text".to_string(),
        text,
        text_html: None,
        tool_name: None,
        tool_call_id: None,
        tool_input: None,
        tool_output: None,
        tool_status: None,
        tool_title: None,
    }]
}

fn content_item_to_projected_part(
    msg_id: &str,
    idx: usize,
    item: &serde_json::Value,
) -> Option<ProjectedChatMessagePart> {
    let obj = item.as_object()?;
    let part_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("text");
    let id = format!("{}:part:{}", msg_id, idx);
    let base = |part_type: &str| ProjectedChatMessagePart {
        id: id.clone(),
        part_type: part_type.to_string(),
        text: None,
        text_html: None,
        tool_name: None,
        tool_call_id: None,
        tool_input: None,
        tool_output: None,
        tool_status: None,
        tool_title: None,
    };
    match part_type {
        "thinking" | "reasoning" => {
            let mut part = base("thinking");
            part.text = obj
                .get("thinking")
                .or_else(|| obj.get("text"))
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            Some(part)
        }
        "tool_call" | "toolCall" | "tool_use" => {
            let mut part = base("tool_call");
            part.tool_name = obj
                .get("name")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            part.tool_call_id = obj
                .get("tool_call_id")
                .or_else(|| obj.get("toolCallId"))
                .or_else(|| obj.get("id"))
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            part.tool_input = obj.get("input").or_else(|| obj.get("arguments")).cloned();
            part.tool_status = Some("success".to_string());
            Some(part)
        }
        "tool_result" | "toolResult" => {
            let mut part = base("tool_result");
            part.tool_name = obj
                .get("name")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            part.tool_call_id = obj
                .get("tool_call_id")
                .or_else(|| obj.get("toolCallId"))
                .or_else(|| obj.get("tool_use_id"))
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            part.tool_output = obj.get("output").or_else(|| obj.get("content")).cloned();
            part.tool_status = Some(
                if obj
                    .get("is_error")
                    .or_else(|| obj.get("isError"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    "error"
                } else {
                    "success"
                }
                .to_string(),
            );
            Some(part)
        }
        _ => {
            let mut part = base("text");
            part.text = obj
                .get("text")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            Some(part)
        }
    }
}

fn row_to_projected_message(
    _idx: usize,
    session_id: &str,
    row: sqlx::sqlite::SqliteRow,
) -> ProjectedChatMessage {
    let msg_id: String = row.get("message_id");
    let parent_id: Option<String> = row.try_get("parent_turn_id").ok();
    let created_at: i64 = row.try_get("created_at_ms").unwrap_or(0);
    let fallback_content: Option<String> = row.try_get("content").ok();
    let json_payload: Option<String> = row.try_get("json_payload").ok();
    let fallback_client_id = json_payload
        .as_deref()
        .and_then(extract_client_id_from_payload_json);
    let role: String = row.get("role");

    let parts = projected_parts_from_payload(&msg_id, fallback_content, json_payload.as_deref());

    ProjectedChatMessage {
        id: msg_id.clone(),
        session_id: session_id.to_string(),
        role,
        created_at,
        completed_at: Some(created_at),
        parent_id,
        model_id: None,
        provider_id: None,
        agent: None,
        summary_title: None,
        tokens_input: None,
        tokens_output: None,
        tokens_reasoning: None,
        cost: None,
        client_id: fallback_client_id,
        parts,
    }
}

#[cfg(test)]
mod tests {
    use super::extract_client_id_from_payload_json;

    #[test]
    fn payload_client_id_extraction_handles_snake_and_camel_case() {
        let snake = r#"{"role":"user","extra":{"client_id":"cid-snake"}}"#;
        assert_eq!(
            extract_client_id_from_payload_json(snake).as_deref(),
            Some("cid-snake")
        );

        let camel = r#"{"role":"user","extra":{"clientId":"cid-camel"}}"#;
        assert_eq!(
            extract_client_id_from_payload_json(camel).as_deref(),
            Some("cid-camel")
        );

        let root = r#"{"role":"user","clientId":"cid-root"}"#;
        assert_eq!(
            extract_client_id_from_payload_json(root).as_deref(),
            Some("cid-root")
        );
    }
}
