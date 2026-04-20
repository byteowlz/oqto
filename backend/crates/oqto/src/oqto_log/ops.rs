use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

static OQTO_LOG_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations_oqto_log");

#[derive(Debug, Default, Clone)]
pub struct OpsSummary {
    pub databases: usize,
    pub sessions: usize,
    pub turns: usize,
    pub messages: usize,
    pub checkpoints: usize,
}

fn list_db_paths(user_home: &Path) -> Vec<PathBuf> {
    let root = user_home
        .join(".local")
        .join("share")
        .join("oqto")
        .join("oqto-log");
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let db = dir.join("oqto-log.sqlite");
        if db.exists() {
            out.push(db);
        }
    }
    out
}

pub async fn diagnostics(user_home: &Path) -> Result<OpsSummary> {
    let mut summary = OpsSummary::default();
    let dbs = list_db_paths(user_home);

    for db in dbs {
        let options = SqliteConnectOptions::new().filename(&db).read_only(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .with_context(|| format!("open db for diagnostics: {}", db.display()))?;

        summary.databases += 1;
        summary.sessions += sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM oqto_log_sessions")
            .fetch_one(&pool)
            .await
            .unwrap_or(0)
            .max(0) as usize;
        summary.turns += sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM oqto_log_turns")
            .fetch_one(&pool)
            .await
            .unwrap_or(0)
            .max(0) as usize;
        summary.messages += sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM oqto_log_messages")
            .fetch_one(&pool)
            .await
            .unwrap_or(0)
            .max(0) as usize;
        summary.checkpoints +=
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM oqto_log_import_checkpoints")
                .fetch_one(&pool)
                .await
                .unwrap_or(0)
                .max(0) as usize;
    }

    Ok(summary)
}

pub async fn reindex_fts(user_home: &Path) -> Result<usize> {
    let dbs = list_db_paths(user_home);
    let mut rebuilt = 0usize;

    for db in dbs {
        let options = SqliteConnectOptions::new().filename(&db);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .with_context(|| format!("open db for reindex: {}", db.display()))?;

        sqlx::query("INSERT INTO oqto_log_message_fts(oqto_log_message_fts) VALUES('rebuild')")
            .execute(&pool)
            .await
            .with_context(|| format!("rebuild fts: {}", db.display()))?;

        rebuilt += 1;
    }

    Ok(rebuilt)
}

#[derive(Debug, Default, Clone)]
pub struct IdentitySyncSummary {
    pub conversations_scanned: usize,
    pub sessions_upserted: usize,
    pub dbs_touched: usize,
}

pub async fn sync_identities_from_hstry(
    user_home: &Path,
    user_id: &str,
) -> Result<IdentitySyncSummary> {
    let hstry_db = user_home
        .join(".local")
        .join("share")
        .join("hstry")
        .join("hstry.db");
    if !hstry_db.exists() {
        return Ok(IdentitySyncSummary::default());
    }

    let hstry_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&hstry_db)
                .read_only(true),
        )
        .await
        .with_context(|| format!("open hstry db for identity sync: {}", hstry_db.display()))?;

    let rows = sqlx::query_as::<_, (Option<String>, Option<String>, Option<String>, String)>(
        r#"
        SELECT external_id, platform_id, workspace, id
        FROM conversations
        WHERE source_id = 'pi'
        "#,
    )
    .fetch_all(&hstry_pool)
    .await
    .context("query hstry conversations for identity sync")?;

    let mut summary = IdentitySyncSummary {
        conversations_scanned: rows.len(),
        ..IdentitySyncSummary::default()
    };

    let mut touched = std::collections::HashSet::new();

    for (external_id, platform_id, workspace, fallback_id) in rows {
        let workspace_id = workspace.unwrap_or_else(|| "global".to_string());
        let session_id = external_id
            .clone()
            .or(platform_id.clone())
            .unwrap_or(fallback_id.clone());

        let db_path =
            crate::oqto_log::paths::resolve_user_home_workspace_db_path(user_home, &workspace_id)?;
        touched.insert(db_path.clone());

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&db_path)
                    .create_if_missing(true),
            )
            .await
            .with_context(|| {
                format!("open oqto-log db for identity sync: {}", db_path.display())
            })?;

        OQTO_LOG_MIGRATOR.run(&pool).await.with_context(|| {
            format!(
                "run oqto-log migrations for identity sync: {}",
                db_path.display()
            )
        })?;

        sqlx::query(
            r#"
            INSERT INTO oqto_log_sessions (
              session_id, platform_id, external_id, user_id, workspace_id, updated_at
            ) VALUES (?, ?, ?, ?, ?, datetime('now'))
            ON CONFLICT(session_id) DO UPDATE SET
              platform_id = COALESCE(excluded.platform_id, oqto_log_sessions.platform_id),
              external_id = COALESCE(excluded.external_id, oqto_log_sessions.external_id),
              user_id = COALESCE(excluded.user_id, oqto_log_sessions.user_id),
              workspace_id = COALESCE(excluded.workspace_id, oqto_log_sessions.workspace_id),
              updated_at = datetime('now')
            "#,
        )
        .bind(&session_id)
        .bind(platform_id.as_deref())
        .bind(external_id.as_deref())
        .bind(user_id)
        .bind(&workspace_id)
        .execute(&pool)
        .await
        .with_context(|| format!("upsert oqto_log session identity: {}", session_id))?;

        summary.sessions_upserted += 1;
    }

    summary.dbs_touched = touched.len();
    Ok(summary)
}

/// Look up an oqto-log session by its external_id (Pi session ID).
/// Scans all workspace databases. Returns (session_id, workspace_id) or `None`.
pub async fn find_session_by_external(
    user_home: &Path,
    external_id: &str,
) -> Option<(String, String)> {
    let dbs = list_db_paths(user_home);

    for db in dbs {
        let options = SqliteConnectOptions::new().filename(&db).read_only(true);
        let pool = match SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
        {
            Ok(pool) => pool,
            Err(_) => continue,
        };

        if let Ok(Some(row)) = sqlx::query_as::<_, (String, String)>(
            "SELECT session_id, COALESCE(workspace_id, '') FROM oqto_log_sessions WHERE external_id = ? LIMIT 1",
        )
        .bind(external_id)
        .fetch_optional(&pool)
        .await
        {
            return Some(row);
        }
    }

    None
}

/// Resolve the Pi external_id for a known oqto-log session identifier.
///
/// Accepts either `session_id` or `platform_id` and returns the non-empty
/// `external_id` when available.
pub async fn find_external_by_session(
    user_home: &Path,
    session_or_platform_id: &str,
) -> Option<String> {
    let dbs = list_db_paths(user_home);

    for db in dbs {
        let options = SqliteConnectOptions::new().filename(&db).read_only(true);
        let pool = match SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
        {
            Ok(pool) => pool,
            Err(_) => continue,
        };

        let row = sqlx::query_scalar::<_, String>(
            "SELECT external_id FROM oqto_log_sessions WHERE (session_id = ? OR platform_id = ?) AND external_id IS NOT NULL AND trim(external_id) != '' LIMIT 1",
        )
        .bind(session_or_platform_id)
        .bind(session_or_platform_id)
        .fetch_optional(&pool)
        .await;

        if let Ok(Some(external_id)) = row {
            return Some(external_id);
        }
    }

    None
}
