use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use oqto_protocol::events::MessageVersion;
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tokio::sync::Mutex;

use crate::protocol::{ChatMessagePartProto, ChatMessageProto, extract_client_id_from_extra};
use oqto_history::oqto_log::paths::resolve_user_home_workspace_db_path;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TurnTreeNode {
    pub turn_id: String,
    pub parent_turn_id: Option<String>,
    pub branch_id: String,
    pub role: String,
    pub turn_version: i64,
}

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
        let mut extra = std::collections::HashMap::new();
        for (k, v) in extra_obj {
            extra.insert(k.clone(), v.clone());
        }
        if let Some(client_id) = extract_client_id_from_extra(&extra) {
            return Some(client_id);
        }
    }

    obj.get("client_id")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("clientId").and_then(|v| v.as_str()))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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
) -> Result<Option<Vec<ChatMessageProto>>> {
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
              t.turn_id AS turn_id,
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
            .map(|(idx, row)| {
                let msg_id: String = row.get("message_id");
                let parent_id: Option<String> = row.try_get("parent_turn_id").ok();
                let created_at: i64 = row.try_get("created_at_ms").unwrap_or(0);
                let fallback_content: Option<String> = row.try_get("content").ok();
                let json_payload: Option<String> = row.try_get("json_payload").ok();

                if let Some(payload) = json_payload.clone()
                    && let Ok(agent_msg) = serde_json::from_str::<oqto_pi::AgentMessage>(&payload)
                {
                    let mut proto =
                        crate::protocol::agent_msg_to_chat_proto(&agent_msg, idx, session_id);
                    proto.id = msg_id;
                    proto.parent_id = parent_id;
                    if created_at > 0 {
                        proto.created_at = created_at;
                        proto.completed_at = Some(created_at);
                    }
                    return proto;
                }

                let fallback_client_id = json_payload
                    .as_deref()
                    .and_then(extract_client_id_from_payload_json);

                let role: String = row.get("role");
                ChatMessageProto {
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
                    parts: vec![ChatMessagePartProto {
                        id: format!("{}:part:0", msg_id),
                        part_type: "text".to_string(),
                        text: fallback_content,
                        text_html: None,
                        tool_name: None,
                        tool_call_id: None,
                        tool_input: None,
                        tool_output: None,
                        tool_status: None,
                        tool_title: None,
                    }],
                }
            })
            .collect();

        return Ok(Some(mapped));
    }

    Ok(None)
}

pub async fn project_session_tree_auto(
    user_home: &Path,
    session_id: &str,
) -> Result<Option<Vec<TurnTreeNode>>> {
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
            .map(|row| TurnTreeNode {
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
) -> Result<Vec<ChatMessageProto>> {
    let pool = open_pool_for_workspace(user_home, workspace_id).await?;
    let mut rows = sqlx::query(
        r#"
        SELECT
          m.message_id AS message_id,
          t.parent_turn_id AS parent_turn_id,
          t.role AS role,
          m.content AS content,
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

    let mapped = rows
        .into_iter()
        .map(|row| {
            let msg_id: String = row.get("message_id");
            let role: String = row.get("role");
            let content: Option<String> = row.try_get("content").ok();
            let created_at: i64 = row.try_get("created_at_ms").unwrap_or(0);
            let parent_id: Option<String> = row.try_get("parent_turn_id").ok();

            ChatMessageProto {
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
                client_id: None,
                parts: vec![ChatMessagePartProto {
                    id: format!("{}:part:0", msg_id),
                    part_type: "text".to_string(),
                    text: content,
                    text_html: None,
                    tool_name: None,
                    tool_call_id: None,
                    tool_input: None,
                    tool_output: None,
                    tool_status: None,
                    tool_title: None,
                }],
            }
        })
        .collect();

    Ok(mapped)
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
