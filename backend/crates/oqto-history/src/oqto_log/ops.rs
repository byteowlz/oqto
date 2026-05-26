use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

static OQTO_LOG_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations_oqto_log");

#[derive(Debug, Clone)]
pub struct OqtoLogSessionRow {
    pub session_id: String,
    pub platform_id: String,
    pub external_id: Option<String>,
    pub user_id: String,
    pub workspace_id: Option<String>,
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

    OQTO_LOG_MIGRATOR.run(&pool).await.with_context(|| {
        format!(
            "run oqto-log migrations for identity upsert: {}",
            db_path.display()
        )
    })?;

    let mut tx = pool.begin().await.context("begin oqto-log identity tx")?;
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
    .bind(session_id)
    .bind(platform_id)
    .bind(external_id)
    .bind(user_id)
    .bind(workspace_id)
    .bind(Option::<&str>::None)
    .execute(&mut *tx)
    .await
    .with_context(|| format!("upsert oqto_log session identity: {}", session_id))?;

    let branch_id = format!("branch:{session_id}:main");
    sqlx::query("INSERT OR IGNORE INTO oqto_log_branches (branch_id, session_id) VALUES (?, ?)")
        .bind(branch_id)
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .context("upsert oqto_log main branch identity")?;

    tx.commit().await.context("commit oqto-log identity tx")?;
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

pub async fn batch_upsert_session_identities(
    user_home: &Path,
    user_id: &str,
    workspace_id: &str,
    identities: &[SessionIdentityInput],
) -> Result<usize> {
    if identities.is_empty() {
        return Ok(0);
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

    OQTO_LOG_MIGRATOR.run(&pool).await.with_context(|| {
        format!(
            "run oqto-log migrations for identity batch upsert: {}",
            db_path.display()
        )
    })?;

    let existing_rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT external_id, session_id, platform_id FROM oqto_log_sessions WHERE external_id IS NOT NULL AND trim(external_id) != ''",
    )
    .fetch_all(&pool)
    .await
    .context("query existing oqto-log identity map")?;
    let existing_by_external: std::collections::HashMap<String, (String, String)> = existing_rows
        .into_iter()
        .map(|(external, session, platform)| (external, (session, platform)))
        .collect();

    let mut tx = pool
        .begin()
        .await
        .context("begin oqto-log identity batch tx")?;
    let mut upserted = 0usize;
    for identity in identities {
        let (session_id, platform_id) = existing_by_external
            .get(&identity.external_id)
            .cloned()
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

        let branch_id = format!("branch:{session_id}:main");
        sqlx::query(
            "INSERT OR IGNORE INTO oqto_log_branches (branch_id, session_id) VALUES (?, ?)",
        )
        .bind(branch_id)
        .bind(&session_id)
        .execute(&mut *tx)
        .await
        .context("batch upsert oqto_log main branch identity")?;
        upserted += 1;
    }

    tx.commit()
        .await
        .context("commit oqto-log identity batch tx")?;
    Ok(upserted)
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
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(SqliteConnectOptions::new().filename(&db))
            .await
            .with_context(|| {
                format!(
                    "open db for out-of-scope identity cleanup: {}",
                    db.display()
                )
            })?;

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
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(SqliteConnectOptions::new().filename(&db))
            .await
            .with_context(|| {
                format!(
                    "open db for session timestamp normalization: {}",
                    db.display()
                )
            })?;

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
              session_id, platform_id, external_id, user_id, workspace_id
            ) VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(session_id) DO UPDATE SET
              platform_id = COALESCE(excluded.platform_id, oqto_log_sessions.platform_id),
              external_id = COALESCE(excluded.external_id, oqto_log_sessions.external_id),
              user_id = COALESCE(excluded.user_id, oqto_log_sessions.user_id),
              workspace_id = COALESCE(excluded.workspace_id, oqto_log_sessions.workspace_id)
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
pub async fn list_sessions(
    user_home: &Path,
    workspace: Option<&str>,
) -> Result<Vec<OqtoLogSessionRow>> {
    let mut sessions = Vec::new();
    for db in list_db_paths(user_home) {
        let options = SqliteConnectOptions::new().filename(&db).read_only(true);
        let pool = match SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
        {
            Ok(pool) => pool,
            Err(_) => continue,
        };

        let rows = if let Some(workspace) = workspace {
            sqlx::query_as::<
                _,
                (
                    String,
                    String,
                    Option<String>,
                    String,
                    Option<String>,
                    String,
                    String,
                    Option<String>,
                    Option<String>,
                    i64,
                ),
            >(
                r#"
                SELECT s.session_id, s.platform_id, s.external_id, s.user_id, s.workspace_id,
                       s.created_at, s.updated_at, s.title,
                       json_extract(s.extensions_json, '$.readable_id') AS readable_id,
                       0 AS messages
                FROM oqto_log_sessions s
                WHERE (s.workspace_id = ? OR s.workspace_id LIKE ?)
                  AND EXISTS (
                      SELECT 1
                      FROM oqto_log_turns t
                      JOIN oqto_log_messages m ON m.turn_id = t.turn_id
                      WHERE t.session_id = s.session_id
                  )
                ORDER BY s.updated_at DESC
                "#,
            )
            .bind(workspace.trim_end_matches('/'))
            .bind(format!("{}/%", workspace.trim_end_matches('/')))
            .fetch_all(&pool)
            .await?
        } else {
            sqlx::query_as::<
                _,
                (
                    String,
                    String,
                    Option<String>,
                    String,
                    Option<String>,
                    String,
                    String,
                    Option<String>,
                    Option<String>,
                    i64,
                ),
            >(
                r#"
                SELECT s.session_id, s.platform_id, s.external_id, s.user_id, s.workspace_id,
                       s.created_at, s.updated_at, s.title,
                       json_extract(s.extensions_json, '$.readable_id') AS readable_id,
                       0 AS messages
                FROM oqto_log_sessions s
                WHERE EXISTS (
                    SELECT 1
                    FROM oqto_log_turns t
                    JOIN oqto_log_messages m ON m.turn_id = t.turn_id
                    WHERE t.session_id = s.session_id
                )
                ORDER BY s.updated_at DESC
                "#,
            )
            .fetch_all(&pool)
            .await?
        };

        sessions.extend(rows.into_iter().map(|row| OqtoLogSessionRow {
            session_id: row.0,
            platform_id: row.1,
            external_id: row.2,
            user_id: row.3,
            workspace_id: row.4,
            created_at: row.5,
            updated_at: row.6,
            title: row.7,
            readable_id: row.8,
            messages: row.9,
        }));
    }
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(sessions)
}

pub async fn get_session(
    user_home: &Path,
    session_or_platform_id: &str,
) -> Result<Option<OqtoLogSessionRow>> {
    for db in list_db_paths(user_home) {
        let options = SqliteConnectOptions::new().filename(&db).read_only(true);
        let pool = match SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
        {
            Ok(pool) => pool,
            Err(_) => continue,
        };
        let row = sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<String>,
                String,
                Option<String>,
                String,
                String,
                Option<String>,
                Option<String>,
                i64,
            ),
        >(
            r#"
            SELECT s.session_id, s.platform_id, s.external_id, s.user_id, s.workspace_id,
                   s.created_at, s.updated_at, s.title,
                   json_extract(s.extensions_json, '$.readable_id') AS readable_id,
                   COUNT(m.message_id) AS messages
            FROM oqto_log_sessions s
            LEFT JOIN oqto_log_turns t ON t.session_id = s.session_id
            LEFT JOIN oqto_log_messages m ON m.turn_id = t.turn_id
            WHERE s.session_id = ? OR s.platform_id = ? OR s.external_id = ?
            GROUP BY s.session_id
            LIMIT 1
            "#,
        )
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
                created_at: row.5,
                updated_at: row.6,
                title: row.7,
                readable_id: row.8,
                messages: row.9,
            }));
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
    Ok(deleted)
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
pub async fn find_session_by_id(
    user_home: &Path,
    session_or_platform_id: &str,
) -> Option<(String, String, Option<String>)> {
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
            return Some(row);
        }
    }

    None
}

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

pub async fn find_platform_by_external(user_home: &Path, external_id: &str) -> Option<String> {
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
    async fn list_get_delete_sessions_use_oqto_log_only() {
        let temp = tempfile::tempdir().expect("temp home");
        append_agent_end_snapshot(
            temp.path(),
            "user-1",
            "/tmp/ws",
            "session-1",
            "platform-1",
            Some("external-1"),
            "external-1",
            &[msg("hello oqto-log")],
        )
        .await
        .expect("seed oqto-log");

        let all = list_sessions(temp.path(), None).await.expect("list all");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].session_id, "session-1");
        assert_eq!(all[0].platform_id, "platform-1");
        assert_eq!(all[0].external_id.as_deref(), Some("external-1"));
        // list_sessions is a sidebar hot path and intentionally avoids
        // message-count joins; callers that need exact counts use get_session.
        assert_eq!(all[0].messages, 0);

        let filtered = list_sessions(temp.path(), Some("/tmp/ws"))
            .await
            .expect("list workspace");
        assert_eq!(filtered.len(), 1);

        let by_platform = get_session(temp.path(), "platform-1")
            .await
            .expect("get by platform")
            .expect("session");
        assert_eq!(by_platform.session_id, "session-1");
        assert_eq!(by_platform.messages, 1);

        assert_eq!(
            find_external_by_session(temp.path(), "platform-1")
                .await
                .as_deref(),
            Some("external-1")
        );

        assert!(
            delete_session(temp.path(), "platform-1")
                .await
                .expect("delete")
        );
        assert!(
            get_session(temp.path(), "platform-1")
                .await
                .expect("get after delete")
                .is_none()
        );
    }
}
