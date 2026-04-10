use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use tokio::sync::Mutex;

use crate::oqto_log::ids::{MessageIdInput, TurnIdInput, derive_message_id, derive_turn_id};
use crate::oqto_log::paths::resolve_user_home_workspace_db_path;
use crate::pi::AgentMessage;

static OQTO_LOG_POOLS: Lazy<Mutex<HashMap<PathBuf, sqlx::SqlitePool>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

static OQTO_LOG_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations_oqto_log");

fn normalize_role(role: &str) -> &'static str {
    if role.eq_ignore_ascii_case("user") {
        "user"
    } else if role.eq_ignore_ascii_case("assistant") || role.eq_ignore_ascii_case("agent") {
        "assistant"
    } else if role.eq_ignore_ascii_case("system") {
        "system"
    } else if role.eq_ignore_ascii_case("tool") || role.eq_ignore_ascii_case("toolresult") {
        "tool"
    } else {
        "assistant"
    }
}

fn extract_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    out.push(text.to_string());
                }
            }
            if out.is_empty() {
                serde_json::to_string(content).unwrap_or_default()
            } else {
                out.join("\n")
            }
        }
        Value::Object(map) => map
            .get("text")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| serde_json::to_string(content).unwrap_or_default()),
        _ => serde_json::to_string(content).unwrap_or_default(),
    }
}

async fn open_workspace_pool(user_home: &Path, workspace_id: &str) -> Result<sqlx::SqlitePool> {
    let db_path = resolve_user_home_workspace_db_path(user_home, workspace_id)?;

    {
        let pools = OQTO_LOG_POOLS.lock().await;
        if let Some(pool) = pools.get(&db_path) {
            return Ok(pool.clone());
        }
    }

    let connect_options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(30));

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await
        .with_context(|| format!("connecting oqto-log db: {}", db_path.display()))?;

    OQTO_LOG_MIGRATOR
        .run(&pool)
        .await
        .context("running oqto-log migrations")?;

    let mut pools = OQTO_LOG_POOLS.lock().await;
    pools.insert(db_path, pool.clone());

    Ok(pool)
}

#[derive(Debug, Clone, Default)]
pub struct AppendStats {
    pub turns_written: usize,
    pub messages_written: usize,
    pub deduped: bool,
    pub snapshot_hash: String,
}

#[derive(Debug, Clone, Default)]
pub struct SessionStats {
    pub turns: usize,
    pub messages: usize,
    pub latest_source_hash: Option<String>,
}

pub async fn append_agent_end_snapshot(
    user_home: &Path,
    user_id: &str,
    workspace_id: &str,
    session_id: &str,
    platform_id: &str,
    external_id: Option<&str>,
    source_session_id: &str,
    messages: &[AgentMessage],
) -> Result<AppendStats> {
    if messages.is_empty() {
        return Ok(AppendStats::default());
    }

    let pool = open_workspace_pool(user_home, workspace_id).await?;
    let mut tx = pool.begin().await.context("begin oqto-log tx")?;

    sqlx::query(
        r#"
        INSERT INTO oqto_log_sessions (
          session_id, platform_id, external_id, user_id, workspace_id, updated_at
        ) VALUES (?, ?, ?, ?, ?, datetime('now'))
        ON CONFLICT(session_id) DO UPDATE SET
          platform_id = excluded.platform_id,
          external_id = COALESCE(excluded.external_id, oqto_log_sessions.external_id),
          user_id = excluded.user_id,
          workspace_id = excluded.workspace_id,
          updated_at = datetime('now')
        "#,
    )
    .bind(session_id)
    .bind(platform_id)
    .bind(external_id)
    .bind(user_id)
    .bind(workspace_id)
    .execute(&mut *tx)
    .await
    .context("upsert oqto_log_sessions")?;

    let branch_id = format!("branch:{}:main", session_id);
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO oqto_log_branches (branch_id, session_id)
        VALUES (?, ?)
        "#,
    )
    .bind(&branch_id)
    .bind(session_id)
    .execute(&mut *tx)
    .await
    .context("upsert oqto_log_branches")?;

    let snapshot_json = serde_json::to_string(messages).unwrap_or_default();
    let snapshot_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(snapshot_json.as_bytes());
        hex::encode(&hasher.finalize()[..16])
    };
    let snapshot_marker = format!("snapshot:{}", snapshot_hash);

    if let Some(last_source_hash) = sqlx::query_scalar::<_, Option<String>>(
        r#"
        SELECT source_hash FROM oqto_log_turns
        WHERE session_id = ?
        ORDER BY turn_version DESC
        LIMIT 1
        "#,
    )
    .bind(session_id)
    .fetch_one(&mut *tx)
    .await
    .ok()
    .flatten()
        && last_source_hash == snapshot_marker
    {
        tx.commit().await.context("commit dedupe oqto-log tx")?;
        return Ok(AppendStats {
            turns_written: 0,
            messages_written: 0,
            deduped: true,
            snapshot_hash,
        });
    }

    let mut turn_version = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(turn_version) FROM oqto_log_turns WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap_or(None)
    .unwrap_or(0);

    let mut parent_turn_id = sqlx::query_scalar::<_, Option<String>>(
        "SELECT head_turn_id FROM oqto_log_branches WHERE branch_id = ?",
    )
    .bind(&branch_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap_or(None);

    for (idx, msg) in messages.iter().enumerate() {
        turn_version += 1;
        let role = normalize_role(&msg.role);
        let source_entry_id = format!("{}:{}", snapshot_hash, idx);

        let turn_id = derive_turn_id(&TurnIdInput {
            session_id,
            branch_id: &branch_id,
            parent_turn_id: parent_turn_id.as_deref(),
            turn_version,
            role,
            source_kind: Some("pi_agent_end"),
            source_session_id: Some(source_session_id),
            source_entry_id: Some(&source_entry_id),
            source_hash: Some(&snapshot_marker),
        });

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO oqto_log_turns (
              turn_id, session_id, branch_id, parent_turn_id, turn_version, role,
              status, source_kind, source_session_id, source_entry_id, source_hash,
              source_timestamp, committed_at
            ) VALUES (?, ?, ?, ?, ?, ?, 'committed', ?, ?, ?, ?, ?, datetime('now'))
            "#,
        )
        .bind(&turn_id)
        .bind(session_id)
        .bind(&branch_id)
        .bind(&parent_turn_id)
        .bind(turn_version)
        .bind(role)
        .bind("pi_agent_end")
        .bind(source_session_id)
        .bind(&source_entry_id)
        .bind(&snapshot_marker)
        .bind(msg.timestamp.map(|v| v.to_string()))
        .execute(&mut *tx)
        .await
        .context("insert oqto_log_turn")?;

        let text = extract_text(&msg.content);
        let json_payload = serde_json::to_string(msg).unwrap_or_default();

        let message_id = derive_message_id(&MessageIdInput {
            turn_id: &turn_id,
            seq: 0,
            kind: "message",
            role: Some(role),
            source_message_id: None,
            content: Some(&text),
        });

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO oqto_log_messages (
              message_id, turn_id, seq, kind, role, content, json_payload
            ) VALUES (?, ?, 0, ?, ?, ?, ?)
            "#,
        )
        .bind(&message_id)
        .bind(&turn_id)
        .bind("message")
        .bind(role)
        .bind(&text)
        .bind(&json_payload)
        .execute(&mut *tx)
        .await
        .context("insert oqto_log_message")?;

        parent_turn_id = Some(turn_id);
    }

    sqlx::query(
        r#"
        UPDATE oqto_log_branches
        SET head_turn_id = ?
        WHERE branch_id = ?
        "#,
    )
    .bind(&parent_turn_id)
    .bind(&branch_id)
    .execute(&mut *tx)
    .await
    .context("update oqto_log_branches head")?;

    tx.commit().await.context("commit oqto-log tx")?;
    Ok(AppendStats {
        turns_written: messages.len(),
        messages_written: messages.len(),
        deduped: false,
        snapshot_hash,
    })
}

pub async fn upsert_import_checkpoint(
    user_home: &Path,
    workspace_id: &str,
    source_kind: &str,
    source_session_id: &str,
    session_id: &str,
    last_offset: Option<i64>,
    last_source_entry_id: Option<&str>,
    last_source_hash: Option<&str>,
) -> Result<()> {
    let pool = open_workspace_pool(user_home, workspace_id).await?;
    let checkpoint_id = format!("cp:{}:{}", source_kind, source_session_id);

    sqlx::query(
        r#"
        INSERT INTO oqto_log_import_checkpoints (
          checkpoint_id, source_kind, source_session_id, session_id,
          last_offset, last_source_entry_id, last_source_hash, schema_version, last_run_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, 1, datetime('now'))
        ON CONFLICT(source_kind, source_session_id) DO UPDATE SET
          session_id = excluded.session_id,
          last_offset = excluded.last_offset,
          last_source_entry_id = excluded.last_source_entry_id,
          last_source_hash = excluded.last_source_hash,
          schema_version = excluded.schema_version,
          last_run_at = datetime('now')
        "#,
    )
    .bind(checkpoint_id)
    .bind(source_kind)
    .bind(source_session_id)
    .bind(session_id)
    .bind(last_offset)
    .bind(last_source_entry_id)
    .bind(last_source_hash)
    .execute(&pool)
    .await
    .context("upsert oqto_log_import_checkpoints")?;

    Ok(())
}

pub async fn read_session_stats(
    user_home: &Path,
    workspace_id: &str,
    session_id: &str,
) -> Result<SessionStats> {
    let pool = open_workspace_pool(user_home, workspace_id).await?;

    let turns =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM oqto_log_turns WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&pool)
            .await
            .unwrap_or(0)
            .max(0) as usize;

    let messages = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM oqto_log_messages m
        JOIN oqto_log_turns t ON t.turn_id = m.turn_id
        WHERE t.session_id = ?
        "#,
    )
    .bind(session_id)
    .fetch_one(&pool)
    .await
    .unwrap_or(0)
    .max(0) as usize;

    let latest_source_hash = sqlx::query_scalar::<_, Option<String>>(
        r#"
        SELECT source_hash FROM oqto_log_turns
        WHERE session_id = ?
        ORDER BY turn_version DESC
        LIMIT 1
        "#,
    )
    .bind(session_id)
    .fetch_one(&pool)
    .await
    .ok()
    .flatten();

    Ok(SessionStats {
        turns,
        messages,
        latest_source_hash,
    })
}
