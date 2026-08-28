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
    ProjectedChatMessage, ProjectedChatMessagePage, ProjectedChatMessagePart, ProjectedTurnTreeNode,
};
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tokio::sync::Mutex;

use crate::oqto_log::paths::resolve_user_home_workspace_db_path;

static PROJECTOR_POOLS: Lazy<Mutex<HashMap<PathBuf, sqlx::SqlitePool>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn projected_created_at_ms_sql() -> &'static str {
    r#"CASE
        WHEN t.source_timestamp IS NOT NULL
          AND t.source_timestamp != ''
          AND t.source_timestamp NOT GLOB '*[^0-9]*'
          AND CAST(t.source_timestamp AS INTEGER) > 0
        THEN CASE
          WHEN CAST(t.source_timestamp AS INTEGER) < 1000000000000
          THEN CAST(t.source_timestamp AS INTEGER) * 1000
          ELSE CAST(t.source_timestamp AS INTEGER)
        END
        ELSE CAST(strftime('%s', COALESCE(t.committed_at, t.created_at)) * 1000 AS INTEGER)
    END"#
}

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

async fn project_session_messages_in_db(
    db_path: &Path,
    session_id: &str,
    limit: Option<usize>,
) -> Option<Vec<ProjectedChatMessage>> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .read_only(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .ok()?;

    let exists =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM oqto_log_sessions WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&pool)
            .await
            .unwrap_or(0);

    if exists <= 0 {
        return None;
    }

    let query = format!(
        r#"
            SELECT
              m.message_id AS message_id,
              t.parent_turn_id AS parent_turn_id,
              t.role AS role,
              m.content AS content,
              m.json_payload AS json_payload,
              {} AS created_at_ms
            FROM oqto_log_turns t
            JOIN oqto_log_messages m ON m.turn_id = t.turn_id
            WHERE t.session_id = ?
            ORDER BY t.turn_version ASC, m.seq ASC
            "#,
        projected_created_at_ms_sql(),
    );
    let mut rows = sqlx::query(&query)
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

    Some(mapped)
}

/// Encode a stable pagination cursor for a projected message position.
///
/// The cursor orders by `(turn_version, seq)`, the same total order the full
/// projection uses, so pages reconcile by position in the durable log — never
/// by text, index, or visible order.
pub fn encode_message_page_cursor(turn_version: i64, seq: i64) -> String {
    format!("v{turn_version}.{seq}")
}

/// Decode a pagination cursor produced by [`encode_message_page_cursor`].
pub fn decode_message_page_cursor(cursor: &str) -> Option<(i64, i64)> {
    let rest = cursor.strip_prefix('v')?;
    let (version, seq) = rest.split_once('.')?;
    Some((version.parse().ok()?, seq.parse().ok()?))
}

async fn project_session_messages_page_in_db(
    db_path: &Path,
    session_id: &str,
    limit: usize,
    before: Option<(i64, i64)>,
) -> Option<ProjectedChatMessagePage> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .read_only(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .ok()?;

    let exists =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM oqto_log_sessions WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&pool)
            .await
            .unwrap_or(0);

    if exists <= 0 {
        return None;
    }

    let cursor_filter = if before.is_some() {
        "AND (t.turn_version < ? OR (t.turn_version = ? AND m.seq < ?))"
    } else {
        ""
    };
    let query = format!(
        r#"
            SELECT
              m.message_id AS message_id,
              t.parent_turn_id AS parent_turn_id,
              t.role AS role,
              m.content AS content,
              m.json_payload AS json_payload,
              {} AS created_at_ms,
              t.turn_version AS turn_version,
              m.seq AS seq
            FROM oqto_log_turns t
            JOIN oqto_log_messages m ON m.turn_id = t.turn_id
            WHERE t.session_id = ?
            {}
            ORDER BY t.turn_version DESC, m.seq DESC
            LIMIT ?
            "#,
        projected_created_at_ms_sql(),
        cursor_filter,
    );

    let mut q = sqlx::query(&query).bind(session_id);
    if let Some((version, seq)) = before {
        q = q.bind(version).bind(version).bind(seq);
    }
    // Fetch one extra row to learn whether an older page exists.
    let mut rows = q
        .bind((limit + 1) as i64)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

    let has_more = rows.len() > limit;
    rows.truncate(limit);

    let next_before = if has_more {
        rows.last().map(|row| {
            let version: i64 = row.try_get("turn_version").unwrap_or(0);
            let seq: i64 = row.try_get("seq").unwrap_or(0);
            encode_message_page_cursor(version, seq)
        })
    } else {
        None
    };

    // Rows were fetched newest-first; present the page oldest-first.
    rows.reverse();
    let messages = rows
        .into_iter()
        .enumerate()
        .map(|(idx, row)| row_to_projected_message(idx, session_id, row))
        .collect();

    Some(ProjectedChatMessagePage {
        messages,
        has_more,
        next_before,
    })
}

/// Project one page of session messages ending at `before` (exclusive), or the
/// newest page when `before` is `None`. Returns `Ok(None)` when the session is
/// not present in any oqto-log store.
pub async fn project_session_messages_page_auto(
    user_home: &Path,
    session_id: &str,
    limit: usize,
    before: Option<&str>,
) -> Result<Option<ProjectedChatMessagePage>> {
    let cursor = match before {
        Some(raw) => Some(
            decode_message_page_cursor(raw)
                .with_context(|| format!("invalid message page cursor: {raw}"))?,
        ),
        None => None,
    };

    if let Some(db_path) = crate::oqto_log::index::lookup_db_path(user_home, session_id).await
        && let Some(page) =
            project_session_messages_page_in_db(&db_path, session_id, limit, cursor).await
    {
        return Ok(Some(page));
    }

    let dirs = list_workspace_hash_dirs(user_home).await;
    for dir in dirs {
        let db_path = dir.join("oqto-log.sqlite");
        if !db_path.exists() {
            continue;
        }
        if let Some(page) =
            project_session_messages_page_in_db(&db_path, session_id, limit, cursor).await
        {
            crate::oqto_log::index::record_scan_hit(user_home, &db_path, session_id, None, None)
                .await;
            return Ok(Some(page));
        }
    }

    Ok(None)
}

pub async fn project_session_messages_auto(
    user_home: &Path,
    session_id: &str,
    limit: Option<usize>,
) -> Result<Option<Vec<ProjectedChatMessage>>> {
    if let Some(db_path) = crate::oqto_log::index::lookup_db_path(user_home, session_id).await
        && let Some(mapped) = project_session_messages_in_db(&db_path, session_id, limit).await
    {
        return Ok(Some(mapped));
    }

    let dirs = list_workspace_hash_dirs(user_home).await;
    for dir in dirs {
        let db_path = dir.join("oqto-log.sqlite");
        if !db_path.exists() {
            continue;
        }
        if let Some(mapped) = project_session_messages_in_db(&db_path, session_id, limit).await {
            crate::oqto_log::index::record_scan_hit(user_home, &db_path, session_id, None, None)
                .await;
            return Ok(Some(mapped));
        }
    }

    Ok(None)
}

async fn project_session_tree_in_db(
    db_path: &Path,
    session_id: &str,
) -> Option<Vec<ProjectedTurnTreeNode>> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .read_only(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .ok()?;

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
        return None;
    }

    Some(
        rows.into_iter()
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
            .collect(),
    )
}

pub async fn project_session_tree_auto(
    user_home: &Path,
    session_id: &str,
) -> Result<Option<Vec<ProjectedTurnTreeNode>>> {
    if let Some(db_path) = crate::oqto_log::index::lookup_db_path(user_home, session_id).await
        && let Some(tree) = project_session_tree_in_db(&db_path, session_id).await
    {
        return Ok(Some(tree));
    }

    let dirs = list_workspace_hash_dirs(user_home).await;
    for dir in dirs {
        let db_path = dir.join("oqto-log.sqlite");
        if !db_path.exists() {
            continue;
        }
        if let Some(tree) = project_session_tree_in_db(&db_path, session_id).await {
            crate::oqto_log::index::record_scan_hit(user_home, &db_path, session_id, None, None)
                .await;
            return Ok(Some(tree));
        }
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

/// Project one page of session messages for a known workspace, oldest-first.
/// Mirrors [`project_session_messages_page_auto`] for a resolved workspace id.
#[allow(dead_code)]
pub async fn project_session_messages_page_for_workspace(
    user_home: &Path,
    workspace_id: &str,
    session_id: &str,
    limit: usize,
    before: Option<&str>,
) -> Result<Option<ProjectedChatMessagePage>> {
    let cursor = match before {
        Some(raw) => Some(
            decode_message_page_cursor(raw)
                .with_context(|| format!("invalid message page cursor: {raw}"))?,
        ),
        None => None,
    };
    let db_path = resolve_user_home_workspace_db_path(user_home, workspace_id)?;
    if !db_path.exists() {
        return Ok(None);
    }
    Ok(project_session_messages_page_in_db(&db_path, session_id, limit, cursor).await)
}

#[allow(dead_code)]
pub async fn project_session_messages_for_workspace(
    user_home: &Path,
    workspace_id: &str,
    session_id: &str,
    limit: Option<usize>,
) -> Result<Vec<ProjectedChatMessage>> {
    let pool = open_pool_for_workspace(user_home, workspace_id).await?;
    let query = format!(
        r#"
        SELECT
          m.message_id AS message_id,
          t.parent_turn_id AS parent_turn_id,
          t.role AS role,
          m.content AS content,
          m.json_payload AS json_payload,
          {} AS created_at_ms
        FROM oqto_log_turns t
        JOIN oqto_log_messages m ON m.turn_id = t.turn_id
        WHERE t.session_id = ?
        ORDER BY t.turn_version ASC, m.seq ASC
        "#,
        projected_created_at_ms_sql(),
    );
    let mut rows = sqlx::query(&query)
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
    role: &str,
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
    if matches!(role, "tool" | "toolResult") {
        let tool_call_id = value
            .get("toolCallId")
            .or_else(|| value.get("tool_call_id"))
            .and_then(|item| item.as_str())
            .map(ToString::to_string);
        let tool_name = value
            .get("toolName")
            .or_else(|| value.get("tool_name"))
            .and_then(|item| item.as_str())
            .map(ToString::to_string);
        let is_error = value
            .get("isError")
            .or_else(|| value.get("is_error"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        return vec![ProjectedChatMessagePart {
            id: format!("{msg_id}:part:0"),
            part_type: "tool_result".to_string(),
            text: None,
            text_html: None,
            tool_name,
            tool_call_id,
            tool_input: None,
            tool_output: Some(content.clone()),
            tool_status: Some(if is_error { "error" } else { "success" }.to_string()),
            tool_title: None,
        }];
    }
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

    let parts =
        projected_parts_from_payload(&msg_id, &role, fallback_content, json_payload.as_deref());

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
    use std::collections::HashMap;

    use oqto_pi::AgentMessage;
    use serde_json::Value;

    use oqto_protocol::projection::ProjectedChatMessagePage;

    use super::{extract_client_id_from_payload_json, project_session_messages_for_workspace};
    use crate::oqto_log::store::{PiJsonlMessageRecord, replace_session_with_pi_jsonl_records};

    fn test_message(role: &str, content: &str, timestamp: Option<u64>) -> AgentMessage {
        AgentMessage {
            role: role.to_string(),
            content: Value::String(content.to_string()),
            timestamp,
            tool_call_id: None,
            tool_name: None,
            is_error: None,
            api: None,
            provider: None,
            model: None,
            usage: None,
            stop_reason: None,
            extra: HashMap::new(),
        }
    }

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

    #[tokio::test]
    async fn projection_uses_pi_jsonl_source_timestamps() {
        let temp = tempfile::tempdir().expect("create temp home");
        let user_home = temp.path();
        let workspace_id = "/tmp/oqto-source-timestamp-test";
        let session_id = "session-source-timestamps";
        let records = vec![
            PiJsonlMessageRecord {
                source_entry_id: "entry-user".to_string(),
                parent_source_entry_id: None,
                source_sequence: 0,
                message: test_message("user", "hi", Some(1_779_363_330_601)),
            },
            PiJsonlMessageRecord {
                source_entry_id: "entry-assistant".to_string(),
                parent_source_entry_id: Some("entry-user".to_string()),
                source_sequence: 1,
                message: test_message("assistant", "hello", Some(1_779_363_330_656)),
            },
        ];

        replace_session_with_pi_jsonl_records(
            user_home,
            "user-1",
            workspace_id,
            session_id,
            "oqto-platform-1",
            Some("external-1"),
            "external-1",
            &records,
        )
        .await
        .expect("replace session from Pi JSONL records");

        let projected =
            project_session_messages_for_workspace(user_home, workspace_id, session_id, None)
                .await
                .expect("project session messages");

        assert_eq!(projected.len(), 2);
        assert_eq!(projected[0].created_at, 1_779_363_330_601);
        assert_eq!(projected[1].created_at, 1_779_363_330_656);
        assert!(projected[0].created_at < projected[1].created_at);
    }

    #[tokio::test]
    async fn projection_preserves_top_level_tool_result_as_structured_part() {
        let temp = tempfile::tempdir().expect("create temp home");
        let user_home = temp.path();
        let workspace_id = "/tmp/oqto-tool-result-projection-test";
        let session_id = "session-tool-result-projection";
        let records = vec![
            PiJsonlMessageRecord {
                source_entry_id: "entry-assistant".to_string(),
                parent_source_entry_id: None,
                source_sequence: 0,
                message: AgentMessage {
                    role: "assistant".to_string(),
                    content: serde_json::json!([{
                        "type": "tool_use",
                        "id": "call-1",
                        "name": "TodoWrite",
                        "input": {"todos": [{"content": "hidden", "status": "completed"}]}
                    }]),
                    timestamp: Some(1_000),
                    tool_call_id: None,
                    tool_name: None,
                    is_error: None,
                    api: None,
                    provider: None,
                    model: None,
                    usage: None,
                    stop_reason: None,
                    extra: HashMap::new(),
                },
            },
            PiJsonlMessageRecord {
                source_entry_id: "entry-tool".to_string(),
                parent_source_entry_id: Some("entry-assistant".to_string()),
                source_sequence: 1,
                message: AgentMessage {
                    role: "toolResult".to_string(),
                    content: serde_json::json!({"todos": [{"content": "hidden"}]}),
                    timestamp: Some(1_001),
                    tool_call_id: Some("call-1".to_string()),
                    tool_name: Some("TodoWrite".to_string()),
                    is_error: Some(false),
                    api: None,
                    provider: None,
                    model: None,
                    usage: None,
                    stop_reason: None,
                    extra: HashMap::new(),
                },
            },
        ];

        replace_session_with_pi_jsonl_records(
            user_home,
            "user-1",
            workspace_id,
            session_id,
            "oqto-platform-1",
            Some("external-1"),
            "external-1",
            &records,
        )
        .await
        .expect("replace session from Pi JSONL records");

        let projected =
            project_session_messages_for_workspace(user_home, workspace_id, session_id, None)
                .await
                .expect("project session messages");

        assert_eq!(projected.len(), 2);
        assert_eq!(projected[0].parts[0].part_type, "tool_call");
        assert_eq!(
            projected[0].parts[0].tool_call_id.as_deref(),
            Some("call-1")
        );
        assert_eq!(projected[1].parts[0].part_type, "tool_result");
        assert_eq!(
            projected[1].parts[0].tool_call_id.as_deref(),
            Some("call-1")
        );
        assert_eq!(
            projected[1].parts[0].tool_name.as_deref(),
            Some("TodoWrite")
        );
        assert_eq!(
            projected[1].parts[0].tool_status.as_deref(),
            Some("success")
        );
        assert!(projected[1].parts[0].tool_output.is_some());
        assert!(projected[1].parts[0].text.is_none());
    }

    #[tokio::test]
    async fn message_pages_reconcile_by_cursor_without_overlap() {
        let temp = tempfile::tempdir().expect("create temp home");
        let user_home = temp.path();
        let workspace_id = "/tmp/oqto-page-test";
        let session_id = "session-paging";
        let records: Vec<PiJsonlMessageRecord> = (0..5)
            .map(|i| PiJsonlMessageRecord {
                source_entry_id: format!("entry-{i}"),
                parent_source_entry_id: if i == 0 {
                    None
                } else {
                    Some(format!("entry-{}", i - 1))
                },
                source_sequence: i,
                message: test_message("user", &format!("msg-{i}"), Some(1_000 + i as u64)),
            })
            .collect();

        replace_session_with_pi_jsonl_records(
            user_home,
            "user-1",
            workspace_id,
            session_id,
            "oqto-platform-1",
            Some("external-1"),
            "external-1",
            &records,
        )
        .await
        .expect("replace session from Pi JSONL records");

        // Newest page first: messages 3 and 4.
        let first = super::project_session_messages_page_for_workspace(
            user_home,
            workspace_id,
            session_id,
            2,
            None,
        )
        .await
        .expect("first page")
        .expect("session exists");
        let stamp = |page: &ProjectedChatMessagePage, idx: usize| page.messages[idx].created_at;
        assert_eq!(first.messages.len(), 2);
        assert_eq!(stamp(&first, 0), 1_003_000);
        assert_eq!(stamp(&first, 1), 1_004_000);
        assert!(first.has_more);
        let cursor = first.next_before.clone().expect("cursor for older page");

        // Cursor pages reconcile by durable position, never by text or count:
        // the same content re-projected must yield the same cursor semantics.
        let second = super::project_session_messages_page_for_workspace(
            user_home,
            workspace_id,
            session_id,
            2,
            Some(&cursor),
        )
        .await
        .expect("second page")
        .expect("session exists");
        assert_eq!(second.messages.len(), 2);
        assert_eq!(stamp(&second, 0), 1_001_000);
        assert_eq!(stamp(&second, 1), 1_002_000);
        assert!(second.has_more);

        let cursor2 = second.next_before.clone().expect("cursor for last page");
        let third = super::project_session_messages_page_for_workspace(
            user_home,
            workspace_id,
            session_id,
            2,
            Some(&cursor2),
        )
        .await
        .expect("third page")
        .expect("session exists");
        assert_eq!(third.messages.len(), 1);
        assert_eq!(stamp(&third, 0), 1_000_000);
        assert!(!third.has_more);
        assert!(third.next_before.is_none());

        // Pages tile the timeline without overlap or gaps.
        let mut stamps: Vec<_> = first
            .messages
            .iter()
            .chain(&second.messages)
            .chain(&third.messages)
            .map(|m| m.created_at)
            .collect();
        stamps.sort();
        assert_eq!(
            stamps,
            vec![1_000_000, 1_001_000, 1_002_000, 1_003_000, 1_004_000]
        );
    }
}
