use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use tokio::sync::Mutex;

use crate::oqto_log::ids::{MessageIdInput, TurnIdInput, derive_message_id, derive_turn_id};
use crate::oqto_log::paths::resolve_user_home_workspace_db_path;
use oqto_pi::AgentMessage;

static OQTO_LOG_POOLS: Lazy<Mutex<HashMap<PathBuf, sqlx::SqlitePool>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

static OQTO_LOG_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations_oqto_log");

const TIMELINE_V1_EXTENSIONS_VERSION: i64 = 20260508001;
const TIMELINE_V1_EXTENSIONS_CHECKSUM: &[u8] = &[
    0x29, 0xc0, 0x34, 0x38, 0xd3, 0x7a, 0x58, 0xb1, 0x17, 0x5f, 0x64, 0xba, 0xef, 0xd1, 0x2e, 0xba,
    0x34, 0x77, 0x84, 0x21, 0xb3, 0x9a, 0x86, 0x0f, 0x23, 0x1f, 0x95, 0x5a, 0x1e, 0x3b, 0x5b, 0xcf,
    0xdf, 0x61, 0xfc, 0x13, 0x2a, 0x61, 0x3d, 0x4d, 0x18, 0x3b, 0xd9, 0xdb, 0x72, 0xf3, 0x62, 0x1e,
];

async fn table_exists(pool: &sqlx::SqlitePool, table_name: &str) -> Result<bool> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table_name)
            .fetch_one(pool)
            .await
            .with_context(|| format!("inspect sqlite table {table_name}"))?;
    Ok(count > 0)
}

async fn column_exists(
    pool: &sqlx::SqlitePool,
    table_name: &str,
    column_name: &str,
) -> Result<bool> {
    let query = format!(
        "SELECT COUNT(1) FROM pragma_table_info('{}') WHERE name = ?",
        table_name.replace('\'', "''")
    );
    let count: i64 = sqlx::query_scalar(&query)
        .bind(column_name)
        .fetch_one(pool)
        .await
        .with_context(|| format!("inspect sqlite column {table_name}.{column_name}"))?;
    Ok(count > 0)
}

pub(crate) async fn repair_accidental_projection_migration_drift(
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    if !table_exists(pool, "_sqlx_migrations").await? {
        return Ok(());
    }

    let migration_checksum: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT checksum FROM _sqlx_migrations WHERE version = ? AND success = 1",
    )
    .bind(TIMELINE_V1_EXTENSIONS_VERSION)
    .fetch_optional(pool)
    .await
    .context("inspect oqto-log timeline migration checksum")?;

    if migration_checksum.as_deref() == Some(TIMELINE_V1_EXTENSIONS_CHECKSUM) {
        return Ok(());
    }

    let has_accidental_table = table_exists(pool, "oqto_log_search_projection_checkpoints").await?;
    if !has_accidental_table {
        return Ok(());
    }

    let has_legacy_table = table_exists(pool, "oqto_log_hstry_projection_checkpoints").await?;
    if !has_legacy_table {
        sqlx::query(
            "ALTER TABLE oqto_log_search_projection_checkpoints RENAME TO oqto_log_hstry_projection_checkpoints",
        )
        .execute(pool)
        .await
        .context("restore immutable timeline projection checkpoint table name")?;
    }

    if column_exists(
        pool,
        "oqto_log_hstry_projection_checkpoints",
        "projection_conversation_id",
    )
    .await?
        && !column_exists(
            pool,
            "oqto_log_hstry_projection_checkpoints",
            "hstry_conversation_id",
        )
        .await?
    {
        sqlx::query(
            "ALTER TABLE oqto_log_hstry_projection_checkpoints RENAME COLUMN projection_conversation_id TO hstry_conversation_id",
        )
        .execute(pool)
        .await
        .context("restore immutable timeline projection checkpoint column name")?;
    }

    sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ? AND success = 1")
        .bind(TIMELINE_V1_EXTENSIONS_CHECKSUM)
        .bind(TIMELINE_V1_EXTENSIONS_VERSION)
        .execute(pool)
        .await
        .context("restore immutable timeline migration checksum")?;

    Ok(())
}

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

pub async fn migrate_db_path(db_path: &Path) -> Result<()> {
    let connect_options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(30));

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(connect_options)
        .await
        .with_context(|| {
            format!(
                "connecting oqto-log db for migration: {}",
                db_path.display()
            )
        })?;

    repair_accidental_projection_migration_drift(&pool)
        .await
        .with_context(|| {
            format!(
                "repairing oqto-log migration metadata: {}",
                db_path.display()
            )
        })?;

    OQTO_LOG_MIGRATOR
        .run(&pool)
        .await
        .with_context(|| format!("running oqto-log migrations: {}", db_path.display()))?;

    Ok(())
}

pub(crate) async fn open_workspace_pool(
    user_home: &Path,
    workspace_id: &str,
) -> Result<sqlx::SqlitePool> {
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

    repair_accidental_projection_migration_drift(&pool)
        .await
        .context("repairing oqto-log migration metadata")?;

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
    /// The Oqto session id actually written to.
    ///
    /// Identity is resolved here (see `canonicalize_session_identity`), so this
    /// need not equal the `session_id` the caller passed. Callers that go on to
    /// read back, checkpoint, or delete-by-identity must use this value: acting
    /// on the id they proposed can address a session that was never written.
    pub session_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct SessionStats {
    pub turns: usize,
    pub messages: usize,
    pub latest_source_hash: Option<String>,
}

fn stable_message_fingerprint(msg: &AgentMessage) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(msg.role.as_bytes());
    hasher.update(b"|");
    hasher.update(msg.tool_call_id.as_deref().unwrap_or("").as_bytes());
    hasher.update(b"|");
    hasher.update(msg.tool_name.as_deref().unwrap_or("").as_bytes());
    hasher.update(b"|");
    hasher.update(if msg.is_error.unwrap_or(false) {
        b"1"
    } else {
        b"0"
    });
    hasher.update(b"|");
    hasher.update(
        serde_json::to_string(&msg.content)
            .unwrap_or_default()
            .as_bytes(),
    );
    hex::encode(&hasher.finalize()[..10])
}

/// True when `id` is a public Oqto session id rather than a harness-native one.
pub fn is_canonical_session_id(id: &str) -> bool {
    id.starts_with("oqto-")
}

/// Canonical public Oqto session id derived from a harness-native external id.
///
/// Deterministic and stable, so an id minted here from the same harness session
/// is always the same id, whichever ingestion path gets there first.
pub fn platform_id_for_external_id(external_id: &str) -> String {
    const NS: uuid::Uuid = uuid::uuid!("7a0b6c2e-74b2-4d2f-a4d3-6d5f7a9d1c31");
    format!("oqto-{}", uuid::Uuid::new_v5(&NS, external_id.as_bytes()))
}

/// Resolve the one public Oqto identity to persist under (ADR-0023).
///
/// `platform_id` is Oqto identity; harness-native ids (pi JSONL uuid, Codex
/// thread id, ...) are binding facts belonging in `external_id` and must never
/// become the session identity. Callers holding only a harness id used to
/// persist under it directly, which minted a second session for a conversation
/// that already had one and split its history across the two.
///
/// Resolution order:
///   1. an existing canonical session bound to this external id wins, even over
///      a canonical `platform_id` the caller proposes. Callers mint by
///      different schemes (frontend random/timestamp, importer uuid-v5), so
///      honouring the proposal would give one conversation a second identity
///      whenever the schemes disagree. Binding-wins is the same rule
///      `batch_upsert_session_identities` already applies;
///   2. else a canonical `platform_id` is taken as the identity;
///   3. else an existing session already stored under the harness id keeps it,
///      because re-minting here would orphan the turns it already owns and
///      split exactly the history this is meant to keep whole;
///   4. else mint deterministically, so a conversation that has no identity yet
///      gets a stable public one rather than a harness id.
async fn canonicalize_session_identity(
    tx: &mut sqlx::SqliteConnection,
    session_id: &str,
    platform_id: &str,
    external_id: Option<&str>,
) -> Result<(String, String)> {
    let binding = external_id
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(platform_id);

    if let Some(existing) = sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT session_id, platform_id
        FROM oqto_log_sessions
        WHERE external_id = ? AND platform_id LIKE 'oqto-%'
        ORDER BY created_at
        LIMIT 1
        "#,
    )
    .bind(binding)
    .fetch_optional(&mut *tx)
    .await
    .context("resolve existing canonical session binding")?
    {
        return Ok(existing);
    }

    if is_canonical_session_id(platform_id) {
        return Ok((session_id.to_string(), platform_id.to_string()));
    }

    if let Some(existing) = sqlx::query_as::<_, (String, String)>(
        "SELECT session_id, platform_id FROM oqto_log_sessions WHERE session_id = ? LIMIT 1",
    )
    .bind(session_id)
    .fetch_optional(&mut *tx)
    .await
    .context("resolve pre-existing session row")?
    {
        return Ok(existing);
    }

    let minted = platform_id_for_external_id(binding);
    Ok((minted.clone(), minted))
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

    let (session_id, platform_id) =
        canonicalize_session_identity(&mut tx, session_id, platform_id, external_id).await?;
    let (session_id, platform_id) = (session_id.as_str(), platform_id.as_str());

    sqlx::query(
        r#"
        INSERT INTO oqto_log_sessions (
          session_id, platform_id, external_id, user_id, workspace_id
        ) VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(session_id) DO UPDATE SET
          platform_id = excluded.platform_id,
          external_id = COALESCE(excluded.external_id, oqto_log_sessions.external_id),
          user_id = excluded.user_id,
          workspace_id = excluded.workspace_id
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
            session_id: session_id.to_string(),
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

    let mut turns_written = 0usize;
    let mut messages_written = 0usize;

    let mut occurrence_by_fingerprint: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for msg in messages {
        let role = normalize_role(&msg.role);
        let fingerprint = stable_message_fingerprint(msg);
        let occ = occurrence_by_fingerprint
            .entry(fingerprint.clone())
            .and_modify(|v| *v += 1)
            .or_insert(0);
        let source_entry_id = format!("{}:{}", fingerprint, *occ);

        if let Some(existing_turn_id) = sqlx::query_scalar::<_, Option<String>>(
            r#"
            SELECT turn_id
            FROM oqto_log_turns
            WHERE source_kind = 'pi_agent_end'
              AND source_session_id = ?
              AND source_entry_id = ?
            LIMIT 1
            "#,
        )
        .bind(source_session_id)
        .bind(&source_entry_id)
        .fetch_one(&mut *tx)
        .await
        .ok()
        .flatten()
        {
            parent_turn_id = Some(existing_turn_id);
            continue;
        }

        turn_version += 1;
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

        turns_written += 1;
        messages_written += 1;
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

    sqlx::query(
        r#"
        UPDATE oqto_log_sessions
        SET
          created_at = COALESCE((
            SELECT datetime(MIN(CAST(source_timestamp AS INTEGER)) / 1000, 'unixepoch')
            FROM oqto_log_turns
            WHERE session_id = ? AND source_timestamp IS NOT NULL AND trim(source_timestamp) != ''
          ), created_at),
          updated_at = COALESCE((
            SELECT datetime(MAX(CAST(source_timestamp AS INTEGER)) / 1000, 'unixepoch')
            FROM oqto_log_turns
            WHERE session_id = ? AND source_timestamp IS NOT NULL AND trim(source_timestamp) != ''
          ), updated_at)
        WHERE session_id = ?
        "#,
    )
    .bind(session_id)
    .bind(session_id)
    .bind(session_id)
    .execute(&mut *tx)
    .await
    .context("update oqto_log_sessions timestamps from source timestamps")?;

    tx.commit().await.context("commit oqto-log tx")?;
    Ok(AppendStats {
        turns_written,
        messages_written,
        deduped: turns_written == 0,
        snapshot_hash,
        session_id: session_id.to_string(),
    })
}

#[derive(Debug, Clone)]
pub struct PiJsonlMessageRecord {
    pub source_entry_id: String,
    pub parent_source_entry_id: Option<String>,
    pub source_sequence: i64,
    pub message: AgentMessage,
}

fn message_source_hash(message: &AgentMessage) -> String {
    use sha2::{Digest, Sha256};
    let payload = serde_json::to_string(message).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(payload.as_bytes());
    format!("message:{}", hex::encode(&hasher.finalize()[..16]))
}

pub async fn replace_session_with_pi_jsonl_records(
    user_home: &Path,
    user_id: &str,
    workspace_id: &str,
    session_id: &str,
    platform_id: &str,
    external_id: Option<&str>,
    source_session_id: &str,
    records: &[PiJsonlMessageRecord],
) -> Result<AppendStats> {
    let messages: Vec<AgentMessage> = records.iter().map(|r| r.message.clone()).collect();
    replace_session_with_snapshot_inner(
        user_home,
        user_id,
        workspace_id,
        session_id,
        platform_id,
        external_id,
        source_session_id,
        &messages,
        Some(records),
    )
    .await
}

pub async fn replace_session_with_snapshot(
    user_home: &Path,
    user_id: &str,
    workspace_id: &str,
    session_id: &str,
    platform_id: &str,
    external_id: Option<&str>,
    source_session_id: &str,
    messages: &[AgentMessage],
) -> Result<AppendStats> {
    replace_session_with_snapshot_inner(
        user_home,
        user_id,
        workspace_id,
        session_id,
        platform_id,
        external_id,
        source_session_id,
        messages,
        None,
    )
    .await
}

async fn replace_session_with_snapshot_inner(
    user_home: &Path,
    user_id: &str,
    workspace_id: &str,
    session_id: &str,
    platform_id: &str,
    external_id: Option<&str>,
    source_session_id: &str,
    messages: &[AgentMessage],
    records: Option<&[PiJsonlMessageRecord]>,
) -> Result<AppendStats> {
    let pool = open_workspace_pool(user_home, workspace_id).await?;
    let mut tx = pool.begin().await.context("begin oqto-log replace tx")?;

    let (session_id, platform_id) =
        canonicalize_session_identity(&mut tx, session_id, platform_id, external_id).await?;
    let (session_id, platform_id) = (session_id.as_str(), platform_id.as_str());

    sqlx::query(
        r#"
        INSERT INTO oqto_log_sessions (
          session_id, platform_id, external_id, user_id, workspace_id
        ) VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(session_id) DO UPDATE SET
          platform_id = excluded.platform_id,
          external_id = COALESCE(excluded.external_id, oqto_log_sessions.external_id),
          user_id = excluded.user_id,
          workspace_id = excluded.workspace_id
        "#,
    )
    .bind(session_id)
    .bind(platform_id)
    .bind(external_id)
    .bind(user_id)
    .bind(workspace_id)
    .execute(&mut *tx)
    .await
    .context("upsert oqto_log_sessions (replace)")?;

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
    .context("upsert oqto_log_branches (replace)")?;

    let snapshot_json = serde_json::to_string(messages).unwrap_or_default();
    let snapshot_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(snapshot_json.as_bytes());
        hex::encode(&hasher.finalize()[..16])
    };

    // Clear existing turns and messages for this session, then re-insert
    // from scratch. Temporarily drop FTS triggers to avoid cascading errors
    // from stale FTS state; the index is rebuilt at the end.
    sqlx::query("DROP TRIGGER IF EXISTS oqto_log_messages_ad")
        .execute(&mut *tx)
        .await
        .context("drop delete trigger (replace)")?;
    sqlx::query("DROP TRIGGER IF EXISTS oqto_log_messages_ai")
        .execute(&mut *tx)
        .await
        .context("drop insert trigger (replace)")?;
    sqlx::query("DROP TRIGGER IF EXISTS oqto_log_messages_au")
        .execute(&mut *tx)
        .await
        .context("drop update trigger (replace)")?;

    sqlx::query("DELETE FROM oqto_log_messages WHERE turn_id IN (SELECT turn_id FROM oqto_log_turns WHERE session_id = ?)")
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .context("delete existing messages (replace)")?;
    sqlx::query("DELETE FROM oqto_log_turns WHERE session_id = ?")
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .context("delete existing turns (replace)")?;

    let mut turn_version: i64 = 0;
    let mut parent_turn_id: Option<String> = None;
    let mut turns_written = 0usize;
    let mut messages_written = 0usize;

    for (idx, msg) in messages.iter().enumerate() {
        let role = normalize_role(&msg.role);
        turn_version += 1;
        let record = records.and_then(|items| items.get(idx));
        let source_entry_id = record
            .map(|r| r.source_entry_id.clone())
            .unwrap_or_else(|| format!("line:{}", idx));
        let source_hash = record
            .map(|r| message_source_hash(&r.message))
            .unwrap_or_else(|| message_source_hash(msg));
        let source_kind = if record.is_some() {
            "pi_jsonl"
        } else {
            "pi_jsonl_bootstrap"
        };

        let turn_id = derive_turn_id(&TurnIdInput {
            session_id,
            branch_id: &branch_id,
            parent_turn_id: parent_turn_id.as_deref(),
            turn_version,
            role,
            source_kind: Some(source_kind),
            source_session_id: Some(source_session_id),
            source_entry_id: Some(&source_entry_id),
            source_hash: Some(&source_hash),
        });

        let turn_inserted = sqlx::query(
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
        .bind(source_kind)
        .bind(source_session_id)
        .bind(&source_entry_id)
        .bind(&source_hash)
        .bind(msg.timestamp.map(|v| v.to_string()))
        .execute(&mut *tx)
        .await
        .context("insert oqto_log_turn (replace)")?
        .rows_affected()
            > 0;

        // If the turn INSERT was ignored (e.g. unique constraint on
        // source_entry_id from another session), skip the message INSERT
        // to avoid a FOREIGN KEY violation. We already deleted this
        // session's turns above, so any IGNORE here is from a cross-session
        // collision -- safe to skip.
        if !turn_inserted {
            turn_version -= 1;
            continue;
        }

        let text = extract_text(&msg.content);
        let json_payload = serde_json::to_string(msg).unwrap_or_default();
        let message_id = derive_message_id(&MessageIdInput {
            turn_id: &turn_id,
            seq: 0,
            kind: "message",
            role: Some(role),
            source_message_id: Some(&source_entry_id),
            content: Some(&text),
        });

        let msg_inserted = sqlx::query(
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
        .context("insert oqto_log_message (replace)")?
        .rows_affected()
            > 0;

        if turn_inserted {
            turns_written += 1;
        }
        if msg_inserted {
            messages_written += 1;
        }
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
    .context("update oqto_log_branches head (replace)")?;

    sqlx::query(
        r#"
        UPDATE oqto_log_sessions
        SET
          created_at = COALESCE((
            SELECT datetime(MIN(CAST(source_timestamp AS INTEGER)) / 1000, 'unixepoch')
            FROM oqto_log_turns
            WHERE session_id = ? AND source_timestamp IS NOT NULL AND trim(source_timestamp) != ''
          ), created_at),
          updated_at = COALESCE((
            SELECT datetime(MAX(CAST(source_timestamp AS INTEGER)) / 1000, 'unixepoch')
            FROM oqto_log_turns
            WHERE session_id = ? AND source_timestamp IS NOT NULL AND trim(source_timestamp) != ''
          ), updated_at)
        WHERE session_id = ?
        "#,
    )
    .bind(session_id)
    .bind(session_id)
    .bind(session_id)
    .execute(&mut *tx)
    .await
    .context("update oqto_log_sessions timestamps from source timestamps (replace)")?;

    // Recreate FTS triggers that were dropped earlier.
    recreate_fts_triggers(&mut tx)
        .await
        .context("recreate FTS triggers (replace)")?;

    // Rebuild the FTS index for correctness after bulk replace.
    sqlx::query("INSERT INTO oqto_log_message_fts(oqto_log_message_fts) VALUES('rebuild')")
        .execute(&mut *tx)
        .await
        .context("rebuild FTS index (replace)")?;

    tx.commit().await.context("commit oqto-log replace tx")?;

    Ok(AppendStats {
        turns_written,
        messages_written,
        deduped: false,
        snapshot_hash,
        session_id: session_id.to_string(),
    })
}

async fn recreate_fts_triggers(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TRIGGER IF NOT EXISTS oqto_log_messages_ai
        AFTER INSERT ON oqto_log_messages
        WHEN NEW.content IS NOT NULL
        BEGIN
            INSERT INTO oqto_log_message_fts (rowid, message_id, turn_id, session_id, role, content)
            VALUES (
                NEW.rowid,
                NEW.message_id,
                NEW.turn_id,
                (SELECT session_id FROM oqto_log_turns WHERE turn_id = NEW.turn_id),
                COALESCE(NEW.role, ''),
                NEW.content
            );
        END
    "#,
    )
    .execute(&mut **tx)
    .await
    .context("recreate insert trigger")?;

    sqlx::query(r#"
        CREATE TRIGGER IF NOT EXISTS oqto_log_messages_ad
        AFTER DELETE ON oqto_log_messages
        BEGIN
            INSERT INTO oqto_log_message_fts (oqto_log_message_fts, rowid, message_id, turn_id, session_id, role, content)
            VALUES ('delete', OLD.rowid, OLD.message_id, OLD.turn_id, '', COALESCE(OLD.role, ''), COALESCE(OLD.content, ''));
        END
    "#)
    .execute(&mut **tx)
    .await
    .context("recreate delete trigger")?;

    sqlx::query(r#"
        CREATE TRIGGER IF NOT EXISTS oqto_log_messages_au
        AFTER UPDATE ON oqto_log_messages
        BEGIN
            INSERT INTO oqto_log_message_fts (oqto_log_message_fts, rowid, message_id, turn_id, session_id, role, content)
            VALUES ('delete', OLD.rowid, OLD.message_id, OLD.turn_id, '', COALESCE(OLD.role, ''), COALESCE(OLD.content, ''));

            INSERT INTO oqto_log_message_fts (rowid, message_id, turn_id, session_id, role, content)
            VALUES (
                NEW.rowid,
                NEW.message_id,
                NEW.turn_id,
                (SELECT session_id FROM oqto_log_turns WHERE turn_id = NEW.turn_id),
                COALESCE(NEW.role, ''),
                COALESCE(NEW.content, '')
            );
        END
    "#)
    .execute(&mut **tx)
    .await
    .context("recreate update trigger")?;

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    fn agent_msg(role: &str, content: &str) -> AgentMessage {
        AgentMessage {
            role: role.to_string(),
            content: Value::String(content.to_string()),
            timestamp: Some(1_779_363_330_601),
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

    async fn sessions_for_external(
        user_home: &Path,
        workspace_id: &str,
        external_id: &str,
    ) -> Vec<(String, String)> {
        let pool = open_workspace_pool(user_home, workspace_id)
            .await
            .expect("open pool");
        sqlx::query_as::<_, (String, String)>(
            "SELECT session_id, platform_id FROM oqto_log_sessions WHERE external_id = ? ORDER BY session_id",
        )
        .bind(external_id)
        .fetch_all(&pool)
        .await
        .expect("query sessions")
    }

    /// One harness session must never yield two Oqto sessions: the live
    /// agent-end path and the JSONL-import path have to land on one identity,
    /// or a reload resolving to the other one shows a split history.
    #[tokio::test]
    async fn one_pi_session_yields_one_oqto_session_across_both_paths() {
        let temp = tempfile::tempdir().expect("temp home");
        let ws = "/tmp/ws-identity";
        let pi_id = "019f5f22-3777-78b7-ab0e-539863fb8232";

        // Frontend-minted identity records the harness id as a binding fact.
        append_agent_end_snapshot(
            temp.path(),
            "user-1",
            ws,
            "oqto-1784007500905-e4b1aea20b3398",
            "oqto-1784007500905-e4b1aea20b3398",
            Some(pi_id),
            pi_id,
            &[agent_msg("user", "first")],
        )
        .await
        .expect("seed canonical session");

        // Live path holding only the harness id must resolve to that identity
        // rather than persisting under the harness id itself.
        append_agent_end_snapshot(
            temp.path(),
            "user-1",
            ws,
            pi_id,
            pi_id,
            Some(pi_id),
            pi_id,
            &[agent_msg("assistant", "second")],
        )
        .await
        .expect("live agent-end under harness id");

        let sessions = sessions_for_external(temp.path(), ws, pi_id).await;
        assert_eq!(
            sessions,
            vec![(
                "oqto-1784007500905-e4b1aea20b3398".to_string(),
                "oqto-1784007500905-e4b1aea20b3398".to_string()
            )],
            "harness id must bind to the existing canonical session, not mint a second one"
        );
    }

    /// Callers mint canonical ids by different schemes: the frontend uses a
    /// random/timestamp id, the importer a uuid-v5 of the external id. A JSONL
    /// rebuild proposing the v5 id for a conversation the frontend already
    /// named must land on the existing session, not open a third identity.
    #[tokio::test]
    async fn rebuild_with_a_different_canonical_scheme_reuses_the_bound_session() {
        let temp = tempfile::tempdir().expect("temp home");
        let ws = "/tmp/ws-scheme";
        let pi_id = "019e456b-fa00-7273-90bd-610e0afb0f03";
        let frontend_id = "oqto-1779281160481-9bc53b6fec002";

        append_agent_end_snapshot(
            temp.path(),
            "user-1",
            ws,
            frontend_id,
            frontend_id,
            Some(pi_id),
            pi_id,
            &[agent_msg("user", "named by the frontend")],
        )
        .await
        .expect("seed frontend-minted session");

        // Importer/bootstrap re-projects the same JSONL under its own scheme.
        let importer_id = platform_id_for_external_id(pi_id);
        assert_ne!(importer_id, frontend_id, "schemes must actually differ");
        append_agent_end_snapshot(
            temp.path(),
            "user-1",
            ws,
            &importer_id,
            &importer_id,
            Some(pi_id),
            pi_id,
            &[agent_msg("assistant", "rebuilt from jsonl")],
        )
        .await
        .expect("rebuild under importer scheme");

        let sessions = sessions_for_external(temp.path(), ws, pi_id).await;
        assert_eq!(
            sessions,
            vec![(frontend_id.to_string(), frontend_id.to_string())],
            "rebuild must reuse the bound identity rather than mint a second"
        );
    }

    /// Identity is resolved here, so callers cannot assume the id they passed
    /// is the id that was written. `AppendStats::session_id` reports what was
    /// actually used; a caller that reads back, checkpoints, or deletes by the
    /// proposed id would address a session that does not exist. (Bootstrap did
    /// exactly that and deleted every session it had just written.)
    #[tokio::test]
    async fn append_reports_the_session_id_it_actually_wrote() {
        let temp = tempfile::tempdir().expect("temp home");
        let ws = "/tmp/ws-reported-id";
        let pi_id = "019f6537-7677-7b2c-922b-2fef2cdf4ad3";

        let stats = append_agent_end_snapshot(
            temp.path(),
            "user-1",
            ws,
            pi_id,
            pi_id,
            Some(pi_id),
            pi_id,
            &[agent_msg("user", "hello")],
        )
        .await
        .expect("append under harness id");

        assert_ne!(
            stats.session_id, pi_id,
            "the harness id must not be the written identity"
        );
        assert_eq!(stats.session_id, platform_id_for_external_id(pi_id));

        // The reported id must address the session that actually exists.
        let found = sessions_for_external(temp.path(), ws, pi_id).await;
        assert_eq!(found, vec![(stats.session_id.clone(), stats.session_id)]);
    }

    /// With no identity yet, a harness id must still not become the identity.
    #[tokio::test]
    async fn harness_id_alone_mints_a_canonical_identity() {
        let temp = tempfile::tempdir().expect("temp home");
        let ws = "/tmp/ws-mint";
        let pi_id = "019f5f22-aaaa-bbbb-cccc-000000000001";

        append_agent_end_snapshot(
            temp.path(),
            "user-1",
            ws,
            pi_id,
            pi_id,
            Some(pi_id),
            pi_id,
            &[agent_msg("user", "only")],
        )
        .await
        .expect("append under harness id");

        let sessions = sessions_for_external(temp.path(), ws, pi_id).await;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].1, platform_id_for_external_id(pi_id));
        assert!(is_canonical_session_id(&sessions[0].1));
        assert_ne!(sessions[0].0, pi_id, "harness id must not be the identity");
    }

    /// Sessions already stored under a harness id predate this rule; re-minting
    /// would orphan the turns they own and split the history being protected.
    #[tokio::test]
    async fn existing_harness_id_session_keeps_its_id() {
        let temp = tempfile::tempdir().expect("temp home");
        let ws = "/tmp/ws-legacy";
        let legacy_id = "legacy-raw-session";

        let pool = open_workspace_pool(temp.path(), ws).await.expect("pool");
        sqlx::query(
            "INSERT INTO oqto_log_sessions (session_id, platform_id, external_id, user_id, workspace_id) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(legacy_id)
        .bind(legacy_id)
        .bind(legacy_id)
        .bind("user-1")
        .bind(ws)
        .execute(&pool)
        .await
        .expect("seed legacy row");

        append_agent_end_snapshot(
            temp.path(),
            "user-1",
            ws,
            legacy_id,
            legacy_id,
            Some(legacy_id),
            legacy_id,
            &[agent_msg("user", "legacy turn")],
        )
        .await
        .expect("append to legacy session");

        let sessions = sessions_for_external(temp.path(), ws, legacy_id).await;
        assert_eq!(
            sessions,
            vec![(legacy_id.to_string(), legacy_id.to_string())],
            "existing harness-id session must keep its id, not gain a second one"
        );
    }

    #[tokio::test]
    async fn repairs_accidental_projection_migration_drift() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let db_path = temp.path().join("oqto-log.sqlite");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&db_path)
                    .create_if_missing(true),
            )
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE _sqlx_migrations (
                version BIGINT PRIMARY KEY,
                description TEXT NOT NULL,
                installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                success BOOLEAN NOT NULL,
                checksum BLOB NOT NULL,
                execution_time BIGINT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (?, 'timeline v1 extensions', 1, ?, 0)",
        )
        .bind(TIMELINE_V1_EXTENSIONS_VERSION)
        .bind(vec![0x71_u8; 48])
        .execute(&pool)
        .await?;
        sqlx::query(
            r#"
            CREATE TABLE oqto_log_search_projection_checkpoints (
                checkpoint_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                projection_conversation_id TEXT,
                last_turn_version INTEGER,
                last_projected_hash TEXT,
                projected_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(&pool)
        .await?;

        repair_accidental_projection_migration_drift(&pool).await?;

        assert!(table_exists(&pool, "oqto_log_hstry_projection_checkpoints").await?);
        assert!(!table_exists(&pool, "oqto_log_search_projection_checkpoints").await?);
        assert!(
            column_exists(
                &pool,
                "oqto_log_hstry_projection_checkpoints",
                "hstry_conversation_id"
            )
            .await?
        );
        let checksum: Vec<u8> =
            sqlx::query_scalar("SELECT checksum FROM _sqlx_migrations WHERE version = ?")
                .bind(TIMELINE_V1_EXTENSIONS_VERSION)
                .fetch_one(&pool)
                .await?;
        assert_eq!(checksum, TIMELINE_V1_EXTENSIONS_CHECKSUM);

        OQTO_LOG_MIGRATOR.run(&pool).await?;
        Ok(())
    }
}
