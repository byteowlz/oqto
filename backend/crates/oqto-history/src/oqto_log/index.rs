//! Session-to-workspace index for oqto-log.
//!
//! oqto-log shards durable history into one SQLite database per workspace.
//! Any lookup that starts from a session id alone (deep links, legacy
//! external ids, cross-workspace search) would otherwise have to probe every
//! workspace database, which costs O(workspaces) pool opens per request.
//! This index maps session/platform/external ids to the owning workspace
//! hash directory so those lookups open exactly one database.
//!
//! The index is strictly an accelerator: it is written best-effort, verified
//! on read, and lazily repaired from full scans. Losing or deleting it only
//! costs speed, never data.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

pub const INDEX_FILE_NAME: &str = "index.sqlite";

fn oqto_log_root(user_home: &Path) -> PathBuf {
    user_home
        .join(".local")
        .join("share")
        .join("oqto")
        .join("oqto-log")
}

fn index_db_path(user_home: &Path) -> PathBuf {
    oqto_log_root(user_home).join(INDEX_FILE_NAME)
}

pub fn index_exists(user_home: &Path) -> bool {
    index_db_path(user_home).exists()
}

async fn open_index(user_home: &Path, create: bool) -> Result<SqlitePool> {
    let path = index_db_path(user_home);
    if !create && !path.exists() {
        anyhow::bail!("oqto-log index does not exist: {}", path.display());
    }
    if create && let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating oqto-log root: {}", parent.display()))?;
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(create)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                .busy_timeout(std::time::Duration::from_secs(5)),
        )
        .await
        .with_context(|| format!("open oqto-log index: {}", path.display()))?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS oqto_log_session_index (
          session_id TEXT PRIMARY KEY,
          platform_id TEXT,
          external_id TEXT,
          workspace_hash TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await
    .context("create oqto-log session index table")?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_oqto_log_session_index_platform ON oqto_log_session_index(platform_id)",
    )
    .execute(&pool)
    .await
    .context("create oqto-log session index platform index")?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_oqto_log_session_index_external ON oqto_log_session_index(external_id)",
    )
    .execute(&pool)
    .await
    .context("create oqto-log session index external index")?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS oqto_log_ingest_cursors (
          external_id TEXT PRIMARY KEY,
          file_size INTEGER NOT NULL,
          file_mtime_ms INTEGER NOT NULL,
          updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
        "#,
    )
    .execute(&pool)
    .await
    .context("create oqto-log ingest cursor table")?;

    Ok(pool)
}

/// Fingerprint of a Pi JSONL session file at the time it was last ingested
/// into oqto-log. Comparing against a fresh `stat()` decides whether the
/// file needs re-ingestion without parsing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngestCursor {
    pub file_size: i64,
    pub file_mtime_ms: i64,
}

pub async fn get_ingest_cursor(user_home: &Path, external_id: &str) -> Option<IngestCursor> {
    let pool = open_index(user_home, false).await.ok()?;
    sqlx::query_as::<_, (i64, i64)>(
        "SELECT file_size, file_mtime_ms FROM oqto_log_ingest_cursors WHERE external_id = ?",
    )
    .bind(external_id)
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten()
    .map(|(file_size, file_mtime_ms)| IngestCursor {
        file_size,
        file_mtime_ms,
    })
}

pub async fn upsert_ingest_cursor(
    user_home: &Path,
    external_id: &str,
    cursor: IngestCursor,
) -> Result<()> {
    let pool = open_index(user_home, true).await?;
    sqlx::query(
        r#"
        INSERT INTO oqto_log_ingest_cursors (external_id, file_size, file_mtime_ms, updated_at)
        VALUES (?, ?, ?, datetime('now'))
        ON CONFLICT(external_id) DO UPDATE SET
          file_size = excluded.file_size,
          file_mtime_ms = excluded.file_mtime_ms,
          updated_at = excluded.updated_at
        "#,
    )
    .bind(external_id)
    .bind(cursor.file_size)
    .bind(cursor.file_mtime_ms)
    .execute(&pool)
    .await
    .context("upsert oqto-log ingest cursor")?;
    Ok(())
}

/// Record that a session lives in the workspace database identified by
/// `workspace_id`. Best-effort: failures are logged by callers at most.
pub async fn upsert_for_workspace_id(
    user_home: &Path,
    workspace_id: &str,
    session_id: &str,
    platform_id: Option<&str>,
    external_id: Option<&str>,
) -> Result<()> {
    let db_path =
        crate::oqto_log::paths::resolve_user_home_workspace_db_path(user_home, workspace_id)?;
    let workspace_hash = db_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .context("derive workspace hash from db path")?
        .to_string();
    upsert_for_workspace_hash(
        user_home,
        &workspace_hash,
        session_id,
        platform_id,
        external_id,
    )
    .await
}

pub async fn upsert_for_workspace_hash(
    user_home: &Path,
    workspace_hash: &str,
    session_id: &str,
    platform_id: Option<&str>,
    external_id: Option<&str>,
) -> Result<()> {
    if session_id.trim().is_empty() {
        return Ok(());
    }
    let pool = open_index(user_home, true).await?;
    sqlx::query(
        r#"
        INSERT INTO oqto_log_session_index (session_id, platform_id, external_id, workspace_hash)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(session_id) DO UPDATE SET
          platform_id = COALESCE(excluded.platform_id, oqto_log_session_index.platform_id),
          external_id = COALESCE(excluded.external_id, oqto_log_session_index.external_id),
          workspace_hash = excluded.workspace_hash
        "#,
    )
    .bind(session_id)
    .bind(platform_id)
    .bind(external_id)
    .bind(workspace_hash)
    .execute(&pool)
    .await
    .context("upsert oqto-log session index entry")?;
    Ok(())
}

/// Batch variant of [`upsert_for_workspace_id`]: one pool open, one tx.
/// Entries are `(session_id, platform_id, external_id)`.
pub async fn upsert_many_for_workspace_id(
    user_home: &Path,
    workspace_id: &str,
    entries: &[(String, Option<String>, Option<String>)],
) -> Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let db_path =
        crate::oqto_log::paths::resolve_user_home_workspace_db_path(user_home, workspace_id)?;
    let workspace_hash = db_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .context("derive workspace hash from db path")?
        .to_string();

    let pool = open_index(user_home, true).await?;
    let mut tx = pool
        .begin()
        .await
        .context("begin oqto-log index batch tx")?;
    for (session_id, platform_id, external_id) in entries {
        if session_id.trim().is_empty() {
            continue;
        }
        sqlx::query(
            r#"
            INSERT INTO oqto_log_session_index (session_id, platform_id, external_id, workspace_hash)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(session_id) DO UPDATE SET
              platform_id = COALESCE(excluded.platform_id, oqto_log_session_index.platform_id),
              external_id = COALESCE(excluded.external_id, oqto_log_session_index.external_id),
              workspace_hash = excluded.workspace_hash
            "#,
        )
        .bind(session_id)
        .bind(platform_id)
        .bind(external_id)
        .bind(&workspace_hash)
        .execute(&mut *tx)
        .await
        .context("batch upsert oqto-log session index entry")?;
    }
    tx.commit()
        .await
        .context("commit oqto-log index batch tx")?;
    Ok(())
}

pub async fn remove_session(user_home: &Path, session_or_platform_id: &str) -> Result<()> {
    let Ok(pool) = open_index(user_home, false).await else {
        return Ok(());
    };
    sqlx::query("DELETE FROM oqto_log_session_index WHERE session_id = ? OR platform_id = ?")
        .bind(session_or_platform_id)
        .bind(session_or_platform_id)
        .execute(&pool)
        .await
        .context("delete oqto-log session index entry")?;
    Ok(())
}

/// Resolve the workspace database that owns `any_id` (session, platform, or
/// external id). Returns `None` on index miss or when the indexed database no
/// longer exists (stale entries are dropped).
pub async fn lookup_db_path(user_home: &Path, any_id: &str) -> Option<PathBuf> {
    let pool = open_index(user_home, false).await.ok()?;
    let row = sqlx::query(
        r#"
        SELECT workspace_hash FROM oqto_log_session_index
        WHERE session_id = ? OR platform_id = ? OR external_id = ?
        LIMIT 1
        "#,
    )
    .bind(any_id)
    .bind(any_id)
    .bind(any_id)
    .fetch_optional(&pool)
    .await
    .ok()??;

    let workspace_hash: String = row.try_get("workspace_hash").ok()?;
    let db = oqto_log_root(user_home)
        .join(&workspace_hash)
        .join(crate::oqto_log::paths::OQTO_LOG_FILE_NAME);
    if db.exists() {
        Some(db)
    } else {
        let _ = sqlx::query("DELETE FROM oqto_log_session_index WHERE workspace_hash = ?")
            .bind(&workspace_hash)
            .execute(&pool)
            .await;
        None
    }
}

/// Record an index entry after a full-scan hit located `db_path`.
pub async fn record_scan_hit(
    user_home: &Path,
    db_path: &Path,
    session_id: &str,
    platform_id: Option<&str>,
    external_id: Option<&str>,
) {
    let Some(workspace_hash) = db_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
    else {
        return;
    };
    if let Err(err) = upsert_for_workspace_hash(
        user_home,
        workspace_hash,
        session_id,
        platform_id,
        external_id,
    )
    .await
    {
        tracing::debug!("oqto-log index scan-hit upsert failed: {err:#}");
    }
}

/// Rebuild the whole index from the workspace databases. Returns the number
/// of indexed sessions.
pub async fn rebuild(user_home: &Path) -> Result<usize> {
    let root = oqto_log_root(user_home);
    let mut indexed = 0usize;

    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(_) => return Ok(0),
    };

    let pool = open_index(user_home, true).await?;
    sqlx::query("DELETE FROM oqto_log_session_index")
        .execute(&pool)
        .await
        .context("clear oqto-log session index")?;

    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Some(workspace_hash) = dir.file_name().and_then(|n| n.to_str()).map(String::from)
        else {
            continue;
        };
        let db = dir.join(crate::oqto_log::paths::OQTO_LOG_FILE_NAME);
        if !db.exists() {
            continue;
        }

        let ws_pool = match SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(SqliteConnectOptions::new().filename(&db).read_only(true))
            .await
        {
            Ok(p) => p,
            Err(_) => continue,
        };

        let rows =
            sqlx::query("SELECT session_id, platform_id, external_id FROM oqto_log_sessions")
                .fetch_all(&ws_pool)
                .await
                .unwrap_or_default();

        for row in rows {
            let session_id: String = match row.try_get("session_id") {
                Ok(v) => v,
                Err(_) => continue,
            };
            let platform_id: Option<String> = row.try_get("platform_id").ok().flatten();
            let external_id: Option<String> = row.try_get("external_id").ok().flatten();
            sqlx::query(
                r#"
                INSERT INTO oqto_log_session_index (session_id, platform_id, external_id, workspace_hash)
                VALUES (?, ?, ?, ?)
                ON CONFLICT(session_id) DO UPDATE SET
                  platform_id = excluded.platform_id,
                  external_id = excluded.external_id,
                  workspace_hash = excluded.workspace_hash
                "#,
            )
            .bind(&session_id)
            .bind(&platform_id)
            .bind(&external_id)
            .bind(&workspace_hash)
            .execute(&pool)
            .await
            .context("insert oqto-log session index row during rebuild")?;
            indexed += 1;
        }
    }

    Ok(indexed)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seed_workspace_db(user_home: &Path, workspace_id: &str, session_id: &str) -> PathBuf {
        let db_path =
            crate::oqto_log::paths::resolve_user_home_workspace_db_path(user_home, workspace_id)
                .expect("db path");
        crate::oqto_log::ops::upsert_session_identity(
            user_home,
            "user1",
            workspace_id,
            session_id,
            Some(session_id),
            Some("ext-1"),
        )
        .await
        .expect("seed session");
        db_path
    }

    #[tokio::test]
    async fn upsert_and_lookup_roundtrip() {
        let temp = tempfile::tempdir().expect("tempdir");
        let db_path = seed_workspace_db(temp.path(), "/ws/one", "oqto-s1").await;

        // upsert_session_identity maintains the index as a side effect.
        let found = lookup_db_path(temp.path(), "oqto-s1").await;
        assert_eq!(found, Some(db_path.clone()));

        let by_external = lookup_db_path(temp.path(), "ext-1").await;
        assert_eq!(by_external, Some(db_path));

        assert_eq!(lookup_db_path(temp.path(), "missing").await, None);
    }

    #[tokio::test]
    async fn stale_entries_are_dropped_when_db_missing() {
        let temp = tempfile::tempdir().expect("tempdir");
        upsert_for_workspace_hash(temp.path(), "deadbeef", "oqto-gone", None, None)
            .await
            .expect("upsert");
        assert_eq!(lookup_db_path(temp.path(), "oqto-gone").await, None);
    }

    #[tokio::test]
    async fn rebuild_indexes_existing_sessions() {
        let temp = tempfile::tempdir().expect("tempdir");
        let db_path = seed_workspace_db(temp.path(), "/ws/two", "oqto-s2").await;

        // Wipe the index, then rebuild from workspace databases.
        std::fs::remove_file(index_db_path(temp.path())).expect("remove index");
        let count = rebuild(temp.path()).await.expect("rebuild");
        assert!(count >= 1);
        assert_eq!(lookup_db_path(temp.path(), "oqto-s2").await, Some(db_path));
    }

    #[tokio::test]
    async fn ingest_cursor_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path();
        assert_eq!(get_ingest_cursor(home, "ext-1").await, None);
        let cursor = IngestCursor {
            file_size: 42,
            file_mtime_ms: 1_700_000_000_000,
        };
        upsert_ingest_cursor(home, "ext-1", cursor)
            .await
            .expect("upsert");
        assert_eq!(get_ingest_cursor(home, "ext-1").await, Some(cursor));
        let newer = IngestCursor {
            file_size: 43,
            file_mtime_ms: 1_700_000_000_500,
        };
        upsert_ingest_cursor(home, "ext-1", newer)
            .await
            .expect("update");
        assert_eq!(get_ingest_cursor(home, "ext-1").await, Some(newer));
    }

    #[tokio::test]
    async fn remove_session_deletes_entry() {
        let temp = tempfile::tempdir().expect("tempdir");
        seed_workspace_db(temp.path(), "/ws/three", "oqto-s3").await;
        assert!(lookup_db_path(temp.path(), "oqto-s3").await.is_some());
        remove_session(temp.path(), "oqto-s3")
            .await
            .expect("remove");
        assert_eq!(lookup_db_path(temp.path(), "oqto-s3").await, None);
    }
}
