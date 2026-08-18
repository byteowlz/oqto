use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use crate::oqto_log::bindings::{
    SessionBindingError, append_pi_session_binding, resolve_pi_session_identity_for_write,
};

static OQTO_LOG_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations_oqto_log");

#[derive(Debug, Clone)]
pub struct OqtoLogSessionRow {
    pub session_id: String,
    pub platform_id: String,
    pub external_id: Option<String>,
    pub user_id: String,
    pub workspace_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub forked_from_entry_id: Option<String>,
    pub forked_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub messages: i64,
    pub title: Option<String>,
    pub readable_id: Option<String>,
}

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

async fn open_migrated_pool(db: &Path, context_label: &str) -> Result<sqlx::SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(db)
                .create_if_missing(true),
        )
        .await
        .with_context(|| format!("open db for {context_label}: {}", db.display()))?;

    crate::oqto_log::store::repair_accidental_projection_migration_drift(&pool)
        .await
        .with_context(|| {
            format!(
                "repair migration metadata for {context_label}: {}",
                db.display()
            )
        })?;

    OQTO_LOG_MIGRATOR
        .run(&pool)
        .await
        .with_context(|| format!("run migrations for {context_label}: {}", db.display()))?;

    Ok(pool)
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

#[derive(Debug, Clone)]
pub struct SessionIdentityInput {
    pub external_id: String,
    pub platform_id: String,
    pub title: Option<String>,
    pub readable_id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

pub async fn upsert_session_identity(
    user_home: &Path,
    user_id: &str,
    workspace_id: &str,
    session_id: &str,
    platform_id: Option<&str>,
    external_id: Option<&str>,
) -> Result<()> {
    let db_path =
        crate::oqto_log::paths::resolve_user_home_workspace_db_path(user_home, workspace_id)?;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true),
        )
        .await
        .with_context(|| {
            format!(
                "open oqto-log db for identity upsert: {}",
                db_path.display()
            )
        })?;

    crate::oqto_log::store::repair_accidental_projection_migration_drift(&pool)
        .await
        .with_context(|| {
            format!(
                "repair oqto-log migration metadata for identity upsert: {}",
                db_path.display()
            )
        })?;

    OQTO_LOG_MIGRATOR.run(&pool).await.with_context(|| {
        format!(
            "run oqto-log migrations for identity upsert: {}",
            db_path.display()
        )
    })?;

    let mut tx = pool.begin().await.context("begin oqto-log identity tx")?;
    let resolved = match external_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(external_id) => resolve_pi_session_identity_for_write(&mut tx, external_id)
            .await
            .context("resolve identity during session upsert")?,
        None => None,
    };
    let (session_id, platform_id) = resolved
        .map(|identity| (identity.session_id, Some(identity.platform_id)))
        .unwrap_or_else(|| (session_id.to_string(), platform_id.map(str::to_string)));
    let platform_id = platform_id.as_deref();

    sqlx::query(
        r#"
        INSERT INTO oqto_log_sessions (
          session_id, platform_id, external_id, user_id, workspace_id, title
        ) VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(session_id) DO UPDATE SET
          platform_id = COALESCE(excluded.platform_id, oqto_log_sessions.platform_id),
          external_id = COALESCE(excluded.external_id, oqto_log_sessions.external_id),
          user_id = COALESCE(excluded.user_id, oqto_log_sessions.user_id),
          workspace_id = COALESCE(excluded.workspace_id, oqto_log_sessions.workspace_id),
          title = COALESCE(excluded.title, oqto_log_sessions.title)
        "#,
    )
    .bind(&session_id)
    .bind(platform_id)
    .bind(external_id)
    .bind(user_id)
    .bind(workspace_id)
    .bind(Option::<&str>::None)
    .execute(&mut *tx)
    .await
    .with_context(|| format!("upsert oqto_log session identity: {}", session_id))?;

    let stored_platform_id: String =
        sqlx::query_scalar("SELECT platform_id FROM oqto_log_sessions WHERE session_id = ?")
            .bind(&session_id)
            .fetch_one(&mut *tx)
            .await
            .context("read stored platform id for session binding")?;
    append_pi_session_binding(
        &mut tx,
        &stored_platform_id,
        external_id,
        "pi-identity-upsert",
    )
    .await
    .context("append pi binding during identity upsert")?;

    let branch_id = format!("branch:{session_id}:main");
    sqlx::query("INSERT OR IGNORE INTO oqto_log_branches (branch_id, session_id) VALUES (?, ?)")
        .bind(branch_id)
        .bind(&session_id)
        .execute(&mut *tx)
        .await
        .context("upsert oqto_log main branch identity")?;

    tx.commit().await.context("commit oqto-log identity tx")?;

    if let Err(err) = crate::oqto_log::index::upsert_for_workspace_id(
        user_home,
        workspace_id,
        &session_id,
        Some(&stored_platform_id),
        external_id,
    )
    .await
    {
        tracing::debug!("oqto-log index upsert failed for {session_id}: {err:#}");
    }
    Ok(())
}

pub async fn update_session_title(
    user_home: &Path,
    session_or_platform_id: &str,
    title: &str,
) -> Result<bool> {
    update_session_title_and_readable_id(user_home, session_or_platform_id, title, None).await
}

pub async fn update_session_title_and_readable_id(
    user_home: &Path,
    session_or_platform_id: &str,
    title: &str,
    readable_id: Option<&str>,
) -> Result<bool> {
    let clean_title = title.trim();
    if clean_title.is_empty() {
        return Ok(false);
    }

    for db in list_db_paths(user_home) {
        let pool = match SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(SqliteConnectOptions::new().filename(&db))
            .await
        {
            Ok(pool) => pool,
            Err(_) => continue,
        };
        let result = sqlx::query(
            r#"
            UPDATE oqto_log_sessions
            SET
              title = ?,
              extensions_json = CASE
                WHEN ? IS NOT NULL THEN json_set(COALESCE(extensions_json, '{}'), '$.readable_id', ?)
                ELSE extensions_json
              END
            WHERE session_id = ? OR platform_id = ? OR external_id = ?
            "#,
        )
        .bind(clean_title)
        .bind(readable_id)
        .bind(readable_id)
        .bind(session_or_platform_id)
        .bind(session_or_platform_id)
        .bind(session_or_platform_id)
        .execute(&pool)
        .await
        .with_context(|| format!("update oqto-log title for session {session_or_platform_id}"))?;
        if result.rows_affected() > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// One external id whose identity could not be upserted because multiple
/// pre-binding legacy rows claim it. The caller owns the authority-backed
/// repair (exact JSONL replace); silently picking a winner here would violate
/// fail-closed identity resolution.
#[derive(Debug, Clone)]
pub struct SessionIdentityConflict {
    pub external_id: String,
    pub session_ids: Vec<String>,
}

#[derive(Debug, Default)]
pub struct SessionIdentityBatchOutcome {
    pub upserted: usize,
    pub conflicts: Vec<SessionIdentityConflict>,
}

pub async fn batch_upsert_session_identities(
    user_home: &Path,
    user_id: &str,
    workspace_id: &str,
    identities: &[SessionIdentityInput],
) -> Result<SessionIdentityBatchOutcome> {
    if identities.is_empty() {
        return Ok(SessionIdentityBatchOutcome::default());
    }

    let db_path =
        crate::oqto_log::paths::resolve_user_home_workspace_db_path(user_home, workspace_id)?;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true),
        )
        .await
        .with_context(|| {
            format!(
                "open oqto-log db for identity batch upsert: {}",
                db_path.display()
            )
        })?;

    crate::oqto_log::store::repair_accidental_projection_migration_drift(&pool)
        .await
        .with_context(|| {
            format!(
                "repair oqto-log migration metadata for identity batch upsert: {}",
                db_path.display()
            )
        })?;

    OQTO_LOG_MIGRATOR.run(&pool).await.with_context(|| {
        format!(
            "run oqto-log migrations for identity batch upsert: {}",
            db_path.display()
        )
    })?;

    let mut tx = pool
        .begin()
        .await
        .context("begin oqto-log identity batch tx")?;
    let mut outcome = SessionIdentityBatchOutcome::default();
    let mut index_entries: Vec<(String, Option<String>, Option<String>)> = Vec::new();
    for identity in identities {
        // A legacy identity conflict is a data fact about one session, not an
        // infrastructure failure: skip that identity and keep the batch alive
        // so one historical split cannot block every other session in the
        // workspace. All other errors still fail (and roll back) the batch.
        let existing =
            match resolve_pi_session_identity_for_write(&mut tx, &identity.external_id).await {
                Ok(existing) => existing,
                Err(SessionBindingError::LegacyConflict {
                    external_id,
                    session_ids,
                }) => {
                    outcome.conflicts.push(SessionIdentityConflict {
                        external_id,
                        session_ids,
                    });
                    continue;
                }
                Err(error) => {
                    return Err(error).context("resolve existing identity during batch upsert");
                }
            };
        let (session_id, platform_id) = existing
            .map(|resolved| (resolved.session_id, resolved.platform_id))
            .unwrap_or_else(|| (identity.platform_id.clone(), identity.platform_id.clone()));

        sqlx::query(
            r#"
            INSERT INTO oqto_log_sessions (
              session_id, platform_id, external_id, user_id, workspace_id, title, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, COALESCE(?, datetime('now')), COALESCE(?, datetime('now')))
            ON CONFLICT(session_id) DO UPDATE SET
              platform_id = COALESCE(excluded.platform_id, oqto_log_sessions.platform_id),
              external_id = COALESCE(excluded.external_id, oqto_log_sessions.external_id),
              user_id = COALESCE(excluded.user_id, oqto_log_sessions.user_id),
              workspace_id = COALESCE(excluded.workspace_id, oqto_log_sessions.workspace_id),
              title = COALESCE(excluded.title, oqto_log_sessions.title),
              extensions_json = CASE
                WHEN ? IS NOT NULL THEN json_set(COALESCE(oqto_log_sessions.extensions_json, '{}'), '$.readable_id', ?)
                ELSE oqto_log_sessions.extensions_json
              END,
              created_at = COALESCE(excluded.created_at, oqto_log_sessions.created_at),
              updated_at = COALESCE(excluded.updated_at, oqto_log_sessions.updated_at)
            "#,
        )
        .bind(&session_id)
        .bind(&platform_id)
        .bind(&identity.external_id)
        .bind(user_id)
        .bind(workspace_id)
        .bind(identity.title.as_deref())
        .bind(identity.created_at.as_deref())
        .bind(identity.updated_at.as_deref())
        .bind(identity.readable_id.as_deref())
        .bind(identity.readable_id.as_deref())
        .execute(&mut *tx)
        .await
        .with_context(|| format!("batch upsert oqto_log session identity: {}", session_id))?;

        append_pi_session_binding(
            &mut tx,
            &platform_id,
            Some(&identity.external_id),
            "pi-identity-batch",
        )
        .await
        .context("append pi binding during identity batch upsert")?;

        let branch_id = format!("branch:{session_id}:main");
        sqlx::query(
            "INSERT OR IGNORE INTO oqto_log_branches (branch_id, session_id) VALUES (?, ?)",
        )
        .bind(branch_id)
        .bind(&session_id)
        .execute(&mut *tx)
        .await
        .context("batch upsert oqto_log main branch identity")?;
        index_entries.push((
            session_id,
            Some(platform_id),
            Some(identity.external_id.clone()),
        ));
        outcome.upserted += 1;
    }

    tx.commit()
        .await
        .context("commit oqto-log identity batch tx")?;

    if let Err(err) = crate::oqto_log::index::upsert_many_for_workspace_id(
        user_home,
        workspace_id,
        &index_entries,
    )
    .await
    {
        tracing::debug!("oqto-log index batch upsert failed: {err:#}");
    }
    Ok(outcome)
}

fn path_is_inside_root(path: &str, root: &Path) -> bool {
    let normalized_path = path.replace('\\', "/").trim_end_matches('/').to_string();
    let normalized_root = root
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    normalized_path == normalized_root
        || normalized_path.starts_with(&format!("{normalized_root}/"))
}

pub async fn delete_identity_only_sessions_outside_workspace(
    user_home: &Path,
    workspace_root: &Path,
) -> Result<usize> {
    let mut deleted = 0usize;
    for db in list_db_paths(user_home) {
        let pool = open_migrated_pool(&db, "out-of-scope identity cleanup").await?;

        let candidates = sqlx::query_as::<_, (String, Option<String>)>(
            r#"
            SELECT s.session_id, s.workspace_id
            FROM oqto_log_sessions s
            WHERE NOT EXISTS (SELECT 1 FROM oqto_log_turns t WHERE t.session_id = s.session_id)
            "#,
        )
        .fetch_all(&pool)
        .await
        .context("query identity-only cleanup candidates")?;

        let mut tx = pool.begin().await.context("begin identity cleanup tx")?;
        for (session_id, workspace_id) in candidates {
            let in_scope = workspace_id
                .as_deref()
                .is_some_and(|workspace| path_is_inside_root(workspace, workspace_root));
            if in_scope {
                continue;
            }
            sqlx::query("DELETE FROM oqto_log_branches WHERE session_id = ?")
                .bind(&session_id)
                .execute(&mut *tx)
                .await
                .context("delete out-of-scope identity branch")?;
            sqlx::query("DELETE FROM oqto_log_sessions WHERE session_id = ?")
                .bind(&session_id)
                .execute(&mut *tx)
                .await
                .context("delete out-of-scope identity session")?;
            deleted += 1;
        }
        tx.commit().await.context("commit identity cleanup tx")?;
    }
    Ok(deleted)
}

pub async fn normalize_session_timestamps_for_workspace(
    user_home: &Path,
    workspace_root: &Path,
) -> Result<usize> {
    let mut normalized = 0usize;
    for db in list_db_paths(user_home) {
        let pool = open_migrated_pool(&db, "session timestamp normalization").await?;

        let mut tx = pool
            .begin()
            .await
            .context("begin session timestamp normalization tx")?;
        let result = sqlx::query(
            r#"
            UPDATE oqto_log_sessions AS s
            SET
              created_at = COALESCE((SELECT MIN(t.created_at) FROM oqto_log_turns t WHERE t.session_id = s.session_id), s.created_at),
              updated_at = COALESCE((SELECT MAX(t.created_at) FROM oqto_log_turns t WHERE t.session_id = s.session_id), s.updated_at)
            WHERE s.workspace_id IS NOT NULL
              AND (s.workspace_id = ? OR s.workspace_id LIKE ?)
              AND EXISTS (SELECT 1 FROM oqto_log_turns t WHERE t.session_id = s.session_id)
            "#,
        )
        .bind(workspace_root.to_string_lossy().trim_end_matches('/').to_string())
        .bind(format!(
            "{}/%",
            workspace_root.to_string_lossy().trim_end_matches('/')
        ))
        .execute(&mut *tx)
        .await
        .context("normalize oqto-log session timestamps from turns")?;

        tx.commit()
            .await
            .context("commit session timestamp normalization tx")?;
        normalized += result.rows_affected() as usize;
    }
    Ok(normalized)
}

/// Look up an oqto-log session by its external_id (Pi session ID).
/// Scans all workspace databases. Returns (session_id, workspace_id) or `None`.
pub async fn list_sessions(
    user_home: &Path,
    workspace: Option<&str>,
) -> Result<Vec<OqtoLogSessionRow>> {
    let mut sessions = Vec::new();
    for db in list_db_paths(user_home) {
        crate::oqto_log::store::migrate_db_path(&db).await?;
        let options = SqliteConnectOptions::new().filename(&db).read_only(true);
        let pool = match SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
        {
            Ok(pool) => pool,
            Err(_) => continue,
        };

        let has_title_column = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM pragma_table_info('oqto_log_sessions') WHERE name = 'title'",
        )
        .fetch_one(&pool)
        .await
        .unwrap_or(0)
            > 0;
        let has_extensions_json_column = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM pragma_table_info('oqto_log_sessions') WHERE name = 'extensions_json'",
        )
        .fetch_one(&pool)
        .await
        .unwrap_or(0)
            > 0;

        let title_expr = if has_title_column {
            "s.title"
        } else {
            "NULL AS title"
        };
        let readable_id_expr = if has_extensions_json_column {
            "json_extract(s.extensions_json, '$.readable_id') AS readable_id"
        } else {
            "NULL AS readable_id"
        };

        let base_select = format!(
            "SELECT s.session_id, s.platform_id, s.external_id, s.user_id, s.workspace_id,\n                    COALESCE((SELECT NULLIF(p.platform_id, '') FROM oqto_log_sessions p WHERE p.session_id = s.parent_session_id), s.parent_session_id) AS parent_session_id, s.forked_from_entry_id, s.forked_at,\n                    s.created_at, s.updated_at, {},\n                    {},\n                    0 AS messages\n             FROM oqto_log_sessions s",
            title_expr, readable_id_expr
        );

        let rows = if let Some(workspace) = workspace {
            let query = format!(
                "{}\n                 WHERE (s.workspace_id = ? OR s.workspace_id LIKE ?)\n                   AND (\n                       s.parent_session_id IS NOT NULL\n                       OR EXISTS (\n                           SELECT 1\n                           FROM oqto_log_turns t\n                           JOIN oqto_log_messages m ON m.turn_id = t.turn_id\n                           WHERE t.session_id = s.session_id\n                       )\n                   )\n                 ORDER BY s.updated_at DESC",
                base_select
            );
            sqlx::query_as::<
                _,
                (
                    String,
                    String,
                    Option<String>,
                    String,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    String,
                    String,
                    Option<String>,
                    Option<String>,
                    i64,
                ),
            >(&query)
            .bind(workspace.trim_end_matches('/'))
            .bind(format!("{}/%", workspace.trim_end_matches('/')))
            .fetch_all(&pool)
            .await?
        } else {
            let query = format!(
                "{}\n                 WHERE s.parent_session_id IS NOT NULL\n                    OR EXISTS (\n                        SELECT 1\n                        FROM oqto_log_turns t\n                        JOIN oqto_log_messages m ON m.turn_id = t.turn_id\n                        WHERE t.session_id = s.session_id\n                    )\n                 ORDER BY s.updated_at DESC",
                base_select
            );
            sqlx::query_as::<
                _,
                (
                    String,
                    String,
                    Option<String>,
                    String,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    Option<String>,
                    String,
                    String,
                    Option<String>,
                    Option<String>,
                    i64,
                ),
            >(&query)
            .fetch_all(&pool)
            .await?
        };

        sessions.extend(rows.into_iter().map(|row| OqtoLogSessionRow {
            session_id: row.0,
            platform_id: row.1,
            external_id: row.2,
            user_id: row.3,
            workspace_id: row.4,
            parent_session_id: row.5,
            forked_from_entry_id: row.6,
            forked_at: row.7,
            created_at: row.8,
            updated_at: row.9,
            title: row.10,
            readable_id: row.11,
            messages: row.12,
        }));
    }
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(sessions)
}

async fn get_session_in_db(
    db: &Path,
    session_or_platform_id: &str,
) -> Result<Option<OqtoLogSessionRow>> {
    crate::oqto_log::store::migrate_db_path(db).await?;
    let options = SqliteConnectOptions::new().filename(db).read_only(true);
    let pool = match SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
    {
        Ok(pool) => pool,
        Err(_) => return Ok(None),
    };
    {
        let has_title_column = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM pragma_table_info('oqto_log_sessions') WHERE name = 'title'",
        )
        .fetch_one(&pool)
        .await
        .unwrap_or(0)
            > 0;
        let has_extensions_json_column = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM pragma_table_info('oqto_log_sessions') WHERE name = 'extensions_json'",
        )
        .fetch_one(&pool)
        .await
        .unwrap_or(0)
            > 0;
        let title_expr = if has_title_column {
            "s.title"
        } else {
            "NULL AS title"
        };
        let readable_id_expr = if has_extensions_json_column {
            "json_extract(s.extensions_json, '$.readable_id') AS readable_id"
        } else {
            "NULL AS readable_id"
        };
        let query = format!(
            "SELECT s.session_id, s.platform_id, s.external_id, s.user_id, s.workspace_id,\n                    COALESCE((SELECT NULLIF(p.platform_id, '') FROM oqto_log_sessions p WHERE p.session_id = s.parent_session_id), s.parent_session_id) AS parent_session_id, s.forked_from_entry_id, s.forked_at,\n                    s.created_at, s.updated_at, {},\n                    {},\n                    COUNT(m.message_id) AS messages\n             FROM oqto_log_sessions s\n             LEFT JOIN oqto_log_turns t ON t.session_id = s.session_id\n             LEFT JOIN oqto_log_messages m ON m.turn_id = t.turn_id\n             WHERE s.session_id = ? OR s.platform_id = ? OR s.external_id = ?\n             GROUP BY s.session_id\n             LIMIT 1",
            title_expr, readable_id_expr
        );
        let row = sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<String>,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                String,
                String,
                Option<String>,
                Option<String>,
                i64,
            ),
        >(&query)
        .bind(session_or_platform_id)
        .bind(session_or_platform_id)
        .bind(session_or_platform_id)
        .fetch_optional(&pool)
        .await?;
        if let Some(row) = row {
            return Ok(Some(OqtoLogSessionRow {
                session_id: row.0,
                platform_id: row.1,
                external_id: row.2,
                user_id: row.3,
                workspace_id: row.4,
                parent_session_id: row.5,
                forked_from_entry_id: row.6,
                forked_at: row.7,
                created_at: row.8,
                updated_at: row.9,
                title: row.10,
                readable_id: row.11,
                messages: row.12,
            }));
        }
    }
    Ok(None)
}

pub async fn get_session(
    user_home: &Path,
    session_or_platform_id: &str,
) -> Result<Option<OqtoLogSessionRow>> {
    if let Some(db) =
        crate::oqto_log::index::lookup_db_path(user_home, session_or_platform_id).await
        && let Ok(Some(row)) = get_session_in_db(&db, session_or_platform_id).await
    {
        return Ok(Some(row));
    }

    for db in list_db_paths(user_home) {
        if let Some(row) = get_session_in_db(&db, session_or_platform_id).await? {
            crate::oqto_log::index::record_scan_hit(
                user_home,
                &db,
                &row.session_id,
                Some(&row.platform_id),
                row.external_id.as_deref(),
            )
            .await;
            return Ok(Some(row));
        }
    }
    Ok(None)
}

pub async fn delete_session(user_home: &Path, session_or_platform_id: &str) -> Result<bool> {
    let mut deleted = false;
    for db in list_db_paths(user_home) {
        let options = SqliteConnectOptions::new().filename(&db);
        let pool = match SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
        {
            Ok(pool) => pool,
            Err(_) => continue,
        };
        let mut tx = pool.begin().await?;
        let ids = sqlx::query_as::<_, (String,)>(
            "SELECT session_id FROM oqto_log_sessions WHERE session_id = ? OR platform_id = ? OR external_id = ?",
        )
        .bind(session_or_platform_id)
        .bind(session_or_platform_id)
        .bind(session_or_platform_id)
        .fetch_all(&mut *tx)
        .await?;
        if ids.is_empty() {
            tx.commit().await?;
            continue;
        }
        sqlx::query("DROP TRIGGER IF EXISTS oqto_log_messages_ad")
            .execute(&mut *tx)
            .await?;
        for (session_id,) in ids {
            sqlx::query("DELETE FROM oqto_log_messages WHERE turn_id IN (SELECT turn_id FROM oqto_log_turns WHERE session_id = ?)")
                .bind(&session_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM oqto_log_turns WHERE session_id = ?")
                .bind(&session_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM oqto_log_branches WHERE session_id = ?")
                .bind(&session_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM oqto_log_sessions WHERE session_id = ?")
                .bind(&session_id)
                .execute(&mut *tx)
                .await?;
            deleted = true;
        }
        sqlx::query("INSERT INTO oqto_log_message_fts(oqto_log_message_fts) VALUES('rebuild')")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }
    if deleted
        && let Err(err) =
            crate::oqto_log::index::remove_session(user_home, session_or_platform_id).await
    {
        tracing::debug!("oqto-log index delete failed for {session_or_platform_id}: {err:#}");
    }
    Ok(deleted)
}

/// A harness session that was persisted under two Oqto identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessSessionSplit {
    pub external_id: String,
    /// The canonical Oqto session that is kept.
    pub keep_session_id: String,
    /// Sessions stored under a harness-native id, superseded by `keep_session_id`.
    pub duplicate_session_ids: Vec<String>,
    /// Whether `keep_session_id` actually holds turns.
    ///
    /// Identity-sync creates canonical rows with no turns, so the canonical side
    /// of a split is not always the side holding the conversation. Collapsing
    /// onto an empty keeper would delete the only copy of that history.
    pub keep_has_turns: bool,
}

/// Find harness sessions that own both a canonical Oqto session and one keyed
/// by the harness id itself. Each pair is one conversation whose history is
/// split across two identities, so a reload resolving to the other one renders
/// an incomplete timeline.
pub async fn find_harness_session_splits(user_home: &Path) -> Result<Vec<HarnessSessionSplit>> {
    let mut splits = Vec::new();
    for db in list_db_paths(user_home) {
        splits.extend(find_harness_session_splits_in_db(&db).await);
    }
    splits.sort_by(|a, b| a.external_id.cmp(&b.external_id));
    Ok(splits)
}

async fn find_harness_session_splits_in_db(db: &Path) -> Vec<HarnessSessionSplit> {
    let options = SqliteConnectOptions::new().filename(db).read_only(true);
    let Ok(pool) = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
    else {
        return Vec::new();
    };

    let rows = sqlx::query_as::<_, (String, String, String)>(
        r#"
        SELECT external_id, session_id, platform_id
        FROM oqto_log_sessions
        WHERE external_id IS NOT NULL AND trim(external_id) != ''
          AND external_id IN (
            SELECT external_id FROM oqto_log_sessions
            WHERE external_id IS NOT NULL AND trim(external_id) != ''
            GROUP BY external_id
            HAVING SUM(platform_id LIKE 'oqto-%') > 0
               AND SUM(platform_id NOT LIKE 'oqto-%') > 0
          )
        ORDER BY external_id, created_at
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();

    let mut by_external: std::collections::HashMap<String, (Option<String>, Vec<String>)> =
        std::collections::HashMap::new();
    for (external_id, session_id, platform_id) in rows {
        let entry = by_external.entry(external_id).or_default();
        if crate::oqto_log::store::is_canonical_session_id(&platform_id) {
            // First canonical row wins, matching how writes resolve bindings.
            if entry.0.is_none() {
                entry.0 = Some(session_id);
            }
        } else {
            entry.1.push(session_id);
        }
    }

    let mut splits = Vec::new();
    for (external_id, (keep, duplicates)) in by_external {
        let Some(keep_session_id) = keep else {
            continue;
        };
        if duplicates.is_empty() {
            continue;
        }
        let keep_turns = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM oqto_log_turns WHERE session_id = ?",
        )
        .bind(&keep_session_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

        splits.push(HarnessSessionSplit {
            external_id,
            keep_session_id,
            duplicate_session_ids: duplicates,
            keep_has_turns: keep_turns > 0,
        });
    }
    splits
}

/// Delete sessions by exact `session_id`, in one transaction with a single FTS
/// rebuild for the whole batch.
///
/// Unlike `delete_session`, this never matches on `platform_id`/`external_id`:
/// the two sides of a split share an external id, so a broader match would take
/// the surviving session with it.
async fn delete_sessions_by_session_id_exact(
    db: &Path,
    session_ids: &[String],
) -> Result<Vec<String>> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(db))
        .await?;
    let mut tx = pool.begin().await?;

    sqlx::query("DROP TRIGGER IF EXISTS oqto_log_messages_ad")
        .execute(&mut *tx)
        .await?;

    let mut deleted = Vec::new();
    for session_id in session_ids {
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM oqto_log_sessions WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_one(&mut *tx)
        .await?;
        if exists == 0 {
            continue;
        }

        sqlx::query("DELETE FROM oqto_log_messages WHERE turn_id IN (SELECT turn_id FROM oqto_log_turns WHERE session_id = ?)")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM oqto_log_turns WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM oqto_log_branches WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM oqto_log_sessions WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;
        deleted.push(session_id.clone());
    }

    sqlx::query("INSERT INTO oqto_log_message_fts(oqto_log_message_fts) VALUES('rebuild')")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(deleted)
}

/// Outcome of collapsing split harness sessions onto their canonical identity.
#[derive(Debug, Default, Clone)]
pub struct UnsplitOutcome {
    /// Harness-id duplicates removed because the canonical session holds the
    /// same conversation.
    pub removed_session_ids: Vec<String>,
    /// Splits left alone because the canonical session holds no turns; the
    /// harness-id row is the only copy and must be re-projected first.
    pub skipped_empty_keepers: Vec<HarnessSessionSplit>,
}

/// Collapse split harness sessions onto their canonical Oqto identity.
///
/// A duplicate is only removed when the canonical session actually holds turns.
/// The canonical row is not always the one with the conversation: identity-sync
/// creates canonical rows with no turns, and for those the harness-id row is the
/// only copy, so collapsing onto it would delete the history outright.
pub async fn unsplit_harness_sessions(user_home: &Path, dry_run: bool) -> Result<UnsplitOutcome> {
    let mut outcome = UnsplitOutcome::default();

    // Resolve and delete per store: a duplicate only ever lives in the store its
    // split was found in, so this stays one pass and one FTS rebuild per store.
    for db in list_db_paths(user_home) {
        let (collapsible, empty_keepers): (Vec<_>, Vec<_>) = find_harness_session_splits_in_db(&db)
            .await
            .into_iter()
            .partition(|split| split.keep_has_turns);

        outcome.skipped_empty_keepers.extend(empty_keepers);

        let duplicates: Vec<String> = collapsible
            .into_iter()
            .flat_map(|split| split.duplicate_session_ids)
            .collect();
        if duplicates.is_empty() {
            continue;
        }
        if dry_run {
            outcome.removed_session_ids.extend(duplicates);
            continue;
        }
        outcome
            .removed_session_ids
            .extend(delete_sessions_by_session_id_exact(&db, &duplicates).await?);
    }

    outcome.removed_session_ids.sort();
    Ok(outcome)
}

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
            r#"
            SELECT s.session_id, COALESCE(s.workspace_id, '')
            FROM oqto_log_sessions s
            LEFT JOIN (
              SELECT session_id, COUNT(*) AS turn_count
              FROM oqto_log_turns
              GROUP BY session_id
            ) tc ON tc.session_id = s.session_id
            WHERE s.external_id = ?
            ORDER BY
              CASE
                WHEN s.session_id = s.platform_id AND s.session_id LIKE 'oqto-%' THEN 0
                WHEN s.session_id = s.platform_id THEN 1
                ELSE 2
              END,
              COALESCE(tc.turn_count, 0) DESC,
              s.updated_at DESC
            LIMIT 1
            "#,
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

pub async fn list_sessions_by_external_in_workspace(
    user_home: &Path,
    workspace_id: &str,
    external_id: &str,
) -> Result<Vec<String>> {
    let db_path =
        crate::oqto_log::paths::resolve_user_home_workspace_db_path(user_home, workspace_id)?;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&db_path)
                .read_only(true),
        )
        .await
        .with_context(|| {
            format!(
                "open oqto-log db for session listing: {}",
                db_path.display()
            )
        })?;

    let rows = sqlx::query_scalar::<_, String>(
        r#"
        SELECT s.session_id
        FROM oqto_log_sessions s
        LEFT JOIN (
          SELECT session_id, COUNT(*) AS turn_count
          FROM oqto_log_turns
          GROUP BY session_id
        ) tc ON tc.session_id = s.session_id
        WHERE s.external_id = ?
        ORDER BY
          CASE
            WHEN s.session_id = s.platform_id AND s.session_id LIKE 'oqto-%' THEN 0
            WHEN s.session_id = s.platform_id THEN 1
            ELSE 2
          END,
          COALESCE(tc.turn_count, 0) DESC,
          s.updated_at DESC
        "#,
    )
    .bind(external_id)
    .fetch_all(&pool)
    .await
    .context("list sessions by external_id")?;

    Ok(rows)
}

pub async fn delete_session_in_workspace(
    user_home: &Path,
    workspace_id: &str,
    session_id: &str,
) -> Result<bool> {
    let db_path =
        crate::oqto_log::paths::resolve_user_home_workspace_db_path(user_home, workspace_id)?;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .with_context(|| format!("open oqto-log db for session delete: {}", db_path.display()))?;

    let mut tx = pool.begin().await.context("begin oqto-log delete tx")?;
    let exists =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM oqto_log_sessions WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&mut *tx)
            .await
            .context("check session exists")?
            > 0;

    if !exists {
        tx.commit().await.context("commit noop delete tx")?;
        return Ok(false);
    }

    sqlx::query("DROP TRIGGER IF EXISTS oqto_log_messages_ad")
        .execute(&mut *tx)
        .await
        .context("drop delete trigger")?;

    sqlx::query("DELETE FROM oqto_log_messages WHERE turn_id IN (SELECT turn_id FROM oqto_log_turns WHERE session_id = ?)")
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .context("delete session messages")?;
    sqlx::query("DELETE FROM oqto_log_turns WHERE session_id = ?")
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .context("delete session turns")?;
    sqlx::query("DELETE FROM oqto_log_branches WHERE session_id = ?")
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .context("delete session branches")?;
    sqlx::query("DELETE FROM oqto_log_sessions WHERE session_id = ?")
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .context("delete session row")?;

    sqlx::query("INSERT INTO oqto_log_message_fts(oqto_log_message_fts) VALUES('rebuild')")
        .execute(&mut *tx)
        .await
        .context("rebuild fts after delete")?;

    tx.commit().await.context("commit session delete")?;

    if let Err(err) = crate::oqto_log::index::remove_session(user_home, session_id).await {
        tracing::debug!("oqto-log index delete failed for {session_id}: {err:#}");
    }
    Ok(true)
}

/// Resolve the Pi external_id for a known oqto-log session identifier.
///
/// Accepts either `session_id` or `platform_id` and returns the non-empty
/// `external_id` when available.
async fn query_session_by_id_in_db(
    db: &Path,
    session_or_platform_id: &str,
) -> Option<(String, String, Option<String>)> {
    let options = SqliteConnectOptions::new().filename(db).read_only(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .ok()?;
    sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT session_id, COALESCE(workspace_id, ''), external_id FROM oqto_log_sessions WHERE (session_id = ? OR platform_id = ?) LIMIT 1",
    )
    .bind(session_or_platform_id)
    .bind(session_or_platform_id)
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten()
}

pub async fn find_session_by_id(
    user_home: &Path,
    session_or_platform_id: &str,
) -> Option<(String, String, Option<String>)> {
    if let Some(db) =
        crate::oqto_log::index::lookup_db_path(user_home, session_or_platform_id).await
        && let Some(row) = query_session_by_id_in_db(&db, session_or_platform_id).await
    {
        return Some(row);
    }

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

        let row = sqlx::query_as::<_, (String, String, Option<String>)>(
            "SELECT session_id, COALESCE(workspace_id, ''), external_id FROM oqto_log_sessions WHERE (session_id = ? OR platform_id = ?) LIMIT 1",
        )
        .bind(session_or_platform_id)
        .bind(session_or_platform_id)
        .fetch_optional(&pool)
        .await;

        if let Ok(Some(row)) = row {
            crate::oqto_log::index::record_scan_hit(
                user_home,
                &db,
                &row.0,
                Some(session_or_platform_id),
                row.2.as_deref(),
            )
            .await;
            return Some(row);
        }
    }

    None
}

async fn source_entry_id_from_db(db: &Path, session_id: &str, entry_id: &str) -> Option<String> {
    let options = SqliteConnectOptions::new().filename(db).read_only(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .ok()?;
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT t.source_entry_id
        FROM oqto_log_turns t
        JOIN oqto_log_messages m ON m.turn_id = t.turn_id
        WHERE t.session_id = ?
          AND (m.message_id = ? OR t.turn_id = ? OR t.source_entry_id = ?)
          AND t.source_entry_id IS NOT NULL
          AND trim(t.source_entry_id) != ''
        LIMIT 1
        "#,
    )
    .bind(session_id)
    .bind(entry_id)
    .bind(entry_id)
    .bind(entry_id)
    .fetch_optional(&pool)
    .await
    .ok()?
}

/// Resolve an oqto-log message/turn ID to the native source entry ID required
/// by harness operations such as Pi fork. Native IDs pass through unchanged.
pub async fn resolve_source_entry_id(
    user_home: &Path,
    session_id: &str,
    entry_id: &str,
) -> Option<String> {
    if let Some(db) = crate::oqto_log::index::lookup_db_path(user_home, session_id).await
        && let Some(source_id) = source_entry_id_from_db(&db, session_id, entry_id).await
    {
        return Some(source_id);
    }

    for db in list_db_paths(user_home) {
        if let Some(source_id) = source_entry_id_from_db(&db, session_id, entry_id).await {
            return Some(source_id);
        }
    }
    None
}

pub async fn find_external_by_session(
    user_home: &Path,
    session_or_platform_id: &str,
) -> Option<String> {
    if let Some(db) =
        crate::oqto_log::index::lookup_db_path(user_home, session_or_platform_id).await
    {
        let options = SqliteConnectOptions::new().filename(&db).read_only(true);
        if let Ok(pool) = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            && let Ok(Some(external_id)) = sqlx::query_scalar::<_, String>(
                "SELECT external_id FROM oqto_log_sessions WHERE (session_id = ? OR platform_id = ?) AND external_id IS NOT NULL AND trim(external_id) != '' LIMIT 1",
            )
            .bind(session_or_platform_id)
            .bind(session_or_platform_id)
            .fetch_optional(&pool)
            .await
        {
            return Some(external_id);
        }
    }

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
            crate::oqto_log::index::record_scan_hit(
                user_home,
                &db,
                session_or_platform_id,
                Some(session_or_platform_id),
                Some(&external_id),
            )
            .await;
            return Some(external_id);
        }
    }

    None
}

pub async fn find_platform_by_external(user_home: &Path, external_id: &str) -> Option<String> {
    if let Some(db) = crate::oqto_log::index::lookup_db_path(user_home, external_id).await {
        let options = SqliteConnectOptions::new().filename(&db).read_only(true);
        if let Ok(pool) = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            && let Ok(Some(platform_id)) = sqlx::query_scalar::<_, String>(
                "SELECT platform_id FROM oqto_log_sessions WHERE external_id = ? AND platform_id IS NOT NULL AND trim(platform_id) != '' LIMIT 1",
            )
            .bind(external_id)
            .fetch_optional(&pool)
            .await
        {
            return Some(platform_id);
        }
    }

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
            "SELECT platform_id FROM oqto_log_sessions WHERE external_id = ? AND platform_id IS NOT NULL AND trim(platform_id) != '' LIMIT 1",
        )
        .bind(external_id)
        .fetch_optional(&pool)
        .await;

        if let Ok(Some(platform_id)) = row {
            crate::oqto_log::index::record_scan_hit(
                user_home,
                &db,
                &platform_id,
                Some(&platform_id),
                Some(external_id),
            )
            .await;
            return Some(platform_id);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use oqto_pi::AgentMessage;
    use serde_json::Value;

    use super::*;
    use crate::oqto_log::store::append_agent_end_snapshot;

    fn msg(content: &str) -> AgentMessage {
        AgentMessage {
            role: "user".to_string(),
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

    #[tokio::test]
    async fn identity_cleanup_migrates_empty_discovered_databases() {
        let temp = tempfile::tempdir().expect("temp home");
        let db_dir = temp
            .path()
            .join(".local/share/oqto/oqto-log/empty-workspace");
        std::fs::create_dir_all(&db_dir).expect("create db dir");
        let db_path = db_dir.join("oqto-log.sqlite");
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&db_path)
                    .create_if_missing(true),
            )
            .await
            .expect("create empty sqlite db")
            .close()
            .await;

        let deleted =
            delete_identity_only_sessions_outside_workspace(temp.path(), Path::new("/tmp/ws"))
                .await
                .expect("cleanup should migrate empty db");
        assert_eq!(deleted, 0);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(SqliteConnectOptions::new().filename(&db_path))
            .await
            .expect("reopen migrated db");
        let sessions_table: i64 = sqlx::query_scalar(
            "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = 'oqto_log_sessions'",
        )
        .fetch_one(&pool)
        .await
        .expect("query schema");
        assert_eq!(sessions_table, 1);
    }

    /// Identity-sync creates canonical rows with no turns. When the canonical
    /// side is empty, the harness-id row is the only copy of the conversation
    /// and collapsing onto the keeper would delete that history outright.
    #[tokio::test]
    async fn unsplit_skips_split_whose_canonical_session_has_no_turns() {
        let temp = tempfile::tempdir().expect("temp home");
        let ws = "/tmp/ws-empty-keeper";
        let pi_id = "019e456b-fa00-7273-90bd-610e0afb0f03";
        let canonical = "oqto-1779281160481-9bc53b6fec002";

        let pool = crate::oqto_log::store::open_workspace_pool(temp.path(), ws)
            .await
            .expect("pool");

        // Identity-only canonical row: bound to the harness session, no turns.
        sqlx::query(
            "INSERT INTO oqto_log_sessions (session_id, platform_id, external_id, user_id, workspace_id) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(canonical)
        .bind(canonical)
        .bind(pi_id)
        .bind("user-1")
        .bind(ws)
        .execute(&pool)
        .await
        .expect("seed identity-only canonical row");

        // The harness-id row carries the actual conversation. Seeded directly
        // because this shape is legacy data written before the identity rule:
        // going through the store now resolves the harness id onto the
        // canonical row instead of reproducing the split.
        sqlx::query(
            "INSERT INTO oqto_log_sessions (session_id, platform_id, external_id, user_id, workspace_id) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(pi_id)
        .bind(pi_id)
        .bind(pi_id)
        .bind("user-1")
        .bind(ws)
        .execute(&pool)
        .await
        .expect("seed harness-id row");
        let branch_id = format!("branch:{pi_id}:main");
        sqlx::query(
            "INSERT OR IGNORE INTO oqto_log_branches (branch_id, session_id) VALUES (?, ?)",
        )
        .bind(&branch_id)
        .bind(pi_id)
        .execute(&pool)
        .await
        .expect("seed branch");
        sqlx::query(
            "INSERT INTO oqto_log_turns (turn_id, session_id, branch_id, turn_version, role, status) VALUES (?, ?, ?, 1, 'user', 'committed')",
        )
        .bind(format!("turn:{pi_id}:1"))
        .bind(pi_id)
        .bind(&branch_id)
        .execute(&pool)
        .await
        .expect("seed harness-id turn");

        let outcome = unsplit_harness_sessions(temp.path(), false)
            .await
            .expect("unsplit");

        assert!(
            outcome.removed_session_ids.is_empty(),
            "must not delete the only copy of the history"
        );
        assert_eq!(outcome.skipped_empty_keepers.len(), 1);
        assert_eq!(outcome.skipped_empty_keepers[0].keep_session_id, canonical);

        let turns = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM oqto_log_turns WHERE session_id = ?",
        )
        .bind(pi_id)
        .fetch_one(&pool)
        .await
        .expect("count turns");
        assert_eq!(turns, 1, "harness-id turns must survive");
    }

    /// The two sides of a split share an external id, so the cleanup must key
    /// strictly on session_id: a broader match would delete the survivor too.
    #[tokio::test]
    async fn unsplit_removes_harness_duplicate_and_keeps_canonical() {
        let temp = tempfile::tempdir().expect("temp home");
        let ws = "/tmp/ws-unsplit";
        let pi_id = "019f5f22-3777-78b7-ab0e-539863fb8232";
        let canonical = "oqto-1784007500905-e4b1aea20b3398";

        // Seed the split directly: a canonical session and a harness-id session
        // for the same conversation, as produced before the identity fix. Both
        // carry turns, which is the case where the duplicate is redundant.
        let pool = crate::oqto_log::store::open_workspace_pool(temp.path(), ws)
            .await
            .expect("pool");
        for (session_id, platform_id) in [(canonical, canonical), (pi_id, pi_id)] {
            sqlx::query(
                "INSERT INTO oqto_log_sessions (session_id, platform_id, external_id, user_id, workspace_id) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(session_id)
            .bind(platform_id)
            .bind(pi_id)
            .bind("user-1")
            .bind(ws)
            .execute(&pool)
            .await
            .expect("seed split row");

            let branch_id = format!("branch:{session_id}:main");
            sqlx::query(
                "INSERT OR IGNORE INTO oqto_log_branches (branch_id, session_id) VALUES (?, ?)",
            )
            .bind(&branch_id)
            .bind(session_id)
            .execute(&pool)
            .await
            .expect("seed branch");
            sqlx::query(
                "INSERT INTO oqto_log_turns (turn_id, session_id, branch_id, turn_version, role, status) VALUES (?, ?, ?, 1, 'user', 'committed')",
            )
            .bind(format!("turn:{session_id}:1"))
            .bind(session_id)
            .bind(&branch_id)
            .execute(&pool)
            .await
            .expect("seed turn");
        }

        let found = find_harness_session_splits(temp.path())
            .await
            .expect("find splits");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].keep_session_id, canonical);
        assert_eq!(found[0].duplicate_session_ids, vec![pi_id.to_string()]);

        let planned = unsplit_harness_sessions(temp.path(), true)
            .await
            .expect("dry run");
        assert_eq!(planned.removed_session_ids, vec![pi_id.to_string()]);
        assert!(planned.skipped_empty_keepers.is_empty());
        assert!(
            get_session(temp.path(), canonical)
                .await
                .expect("dry run keeps canonical")
                .is_some(),
            "dry run must not delete anything"
        );

        let outcome = unsplit_harness_sessions(temp.path(), false)
            .await
            .expect("unsplit");
        assert_eq!(outcome.removed_session_ids, vec![pi_id.to_string()]);

        let remaining = sqlx::query_as::<_, (String,)>(
            "SELECT session_id FROM oqto_log_sessions WHERE external_id = ?",
        )
        .bind(pi_id)
        .fetch_all(&pool)
        .await
        .expect("query remaining");
        assert_eq!(
            remaining,
            vec![(canonical.to_string(),)],
            "only the canonical session may survive"
        );

        assert!(
            find_harness_session_splits(temp.path())
                .await
                .expect("re-scan")
                .is_empty(),
            "unsplit must be idempotent"
        );
    }

    #[tokio::test]
    async fn list_get_delete_sessions_use_oqto_log_only() {
        let temp = tempfile::tempdir().expect("temp home");
        append_agent_end_snapshot(
            temp.path(),
            "user-1",
            "/tmp/ws",
            "oqto-session-1",
            "oqto-platform-1",
            Some("external-1"),
            "external-1",
            &[msg("hello oqto-log")],
        )
        .await
        .expect("seed oqto-log");

        let all = list_sessions(temp.path(), None).await.expect("list all");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].session_id, "oqto-session-1");
        assert_eq!(all[0].platform_id, "oqto-platform-1");
        assert_eq!(all[0].external_id.as_deref(), Some("external-1"));
        // list_sessions is a sidebar hot path and intentionally avoids
        // message-count joins; callers that need exact counts use get_session.
        assert_eq!(all[0].messages, 0);

        let filtered = list_sessions(temp.path(), Some("/tmp/ws"))
            .await
            .expect("list workspace");
        assert_eq!(filtered.len(), 1);

        let by_platform = get_session(temp.path(), "oqto-platform-1")
            .await
            .expect("get by platform")
            .expect("session");
        assert_eq!(by_platform.session_id, "oqto-session-1");
        assert_eq!(by_platform.messages, 1);

        assert_eq!(
            find_external_by_session(temp.path(), "oqto-platform-1")
                .await
                .as_deref(),
            Some("external-1")
        );

        let db =
            crate::oqto_log::paths::resolve_user_home_workspace_db_path(temp.path(), "/tmp/ws")
                .expect("workspace db path");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite:{}", db.display()))
            .await
            .expect("open seeded db");
        let (message_id, source_entry_id): (String, String) = sqlx::query_as(
            "SELECT m.message_id, t.source_entry_id FROM oqto_log_turns t JOIN oqto_log_messages m ON m.turn_id = t.turn_id WHERE t.session_id = ? LIMIT 1",
        )
        .bind("oqto-session-1")
        .fetch_one(&pool)
        .await
        .expect("seeded message identity");
        assert_eq!(
            resolve_source_entry_id(temp.path(), "oqto-session-1", &message_id)
                .await
                .as_deref(),
            Some(source_entry_id.as_str())
        );
        assert_eq!(
            resolve_source_entry_id(temp.path(), "oqto-session-1", &source_entry_id)
                .await
                .as_deref(),
            Some(source_entry_id.as_str()),
            "native source ids must pass through"
        );

        assert!(
            delete_session(temp.path(), "oqto-platform-1")
                .await
                .expect("delete")
        );
        assert!(
            get_session(temp.path(), "oqto-platform-1")
                .await
                .expect("get after delete")
                .is_none()
        );
    }
}
