use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use once_cell::sync::Lazy;
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tokio::sync::Mutex;

/// Default hstry database path, if it exists.
pub fn hstry_db_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("OQTO_HSTRY_DB") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    if let Some(path) = hstry_db_path_from_config()
        && path.exists()
    {
        return Some(path);
    }

    let home = std::env::var("HOME").ok().map(PathBuf::from)?;
    let default = home
        .join(".local")
        .join("share")
        .join("hstry")
        .join("hstry.db");
    default.exists().then_some(default)
}

fn hstry_db_path_from_config() -> Option<PathBuf> {
    let config_dir = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })?;
    let config_path = config_dir.join("hstry").join("config.toml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let parsed: toml::Value = content.parse().ok()?;
    let db_str = parsed.get("database")?.as_str()?;
    Some(PathBuf::from(db_str))
}

/// Extract a stable display project name from a workspace path.
pub fn project_name_from_path(path: &str) -> String {
    if path == "global" || path.is_empty() {
        return "Global".to_string();
    }
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

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

pub async fn resolve_conversation_identity(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    workspace: Option<&str>,
) -> Result<Option<(String, Option<String>)>> {
    let exact = if let Some(workspace) = workspace {
        sqlx::query(
            r#"
            SELECT c.id,
                   c.external_id,
                   c.platform_id,
                   (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id) AS msg_count
            FROM conversations c
            WHERE (c.external_id = ? OR c.platform_id = ? OR c.id = ?)
              AND c.workspace = ?
            ORDER BY COALESCE(c.updated_at, c.created_at) DESC
            LIMIT 1
            "#,
        )
        .bind(session_id)
        .bind(session_id)
        .bind(session_id)
        .bind(workspace)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT c.id,
                   c.external_id,
                   c.platform_id,
                   (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id) AS msg_count
            FROM conversations c
            WHERE c.external_id = ? OR c.platform_id = ? OR c.id = ?
            ORDER BY COALESCE(c.updated_at, c.created_at) DESC
            LIMIT 1
            "#,
        )
        .bind(session_id)
        .bind(session_id)
        .bind(session_id)
        .fetch_optional(pool)
        .await?
    };

    if let Some(row) = exact {
        let id: String = row.get("id");
        let external_id: Option<String> = row.get("external_id");
        let platform_id: Option<String> = row.try_get("platform_id").ok();
        let msg_count: i64 = row.try_get("msg_count").unwrap_or(0);

        if msg_count == 0
            && let Some(pid) = platform_id.as_deref().filter(|p| !p.is_empty())
        {
            let sibling = if let Some(workspace) = workspace {
                sqlx::query(
                    r#"
                    SELECT c.id,
                           c.external_id,
                           (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id) AS msg_count
                    FROM conversations c
                    WHERE c.platform_id = ?
                      AND c.workspace = ?
                    ORDER BY msg_count DESC, COALESCE(c.updated_at, c.created_at) DESC
                    LIMIT 1
                    "#,
                )
                .bind(pid)
                .bind(workspace)
                .fetch_optional(pool)
                .await?
            } else {
                sqlx::query(
                    r#"
                    SELECT c.id,
                           c.external_id,
                           (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id) AS msg_count
                    FROM conversations c
                    WHERE c.platform_id = ?
                    ORDER BY msg_count DESC, COALESCE(c.updated_at, c.created_at) DESC
                    LIMIT 1
                    "#,
                )
                .bind(pid)
                .fetch_optional(pool)
                .await?
            };

            if let Some(sibling_row) = sibling {
                let sibling_count: i64 = sibling_row.try_get("msg_count").unwrap_or(0);
                if sibling_count > 0 {
                    let sid: String = sibling_row.get("id");
                    let sexternal: Option<String> = sibling_row.get("external_id");
                    return Ok(Some((sid, sexternal)));
                }
            }
        }

        return Ok(Some((id, external_id)));
    }

    let readable_rows = if let Some(workspace) = workspace {
        sqlx::query(
            r#"
            SELECT id, external_id
            FROM conversations
            WHERE readable_id = ?
              AND workspace = ?
            ORDER BY COALESCE(updated_at, created_at) DESC
            LIMIT 2
            "#,
        )
        .bind(session_id)
        .bind(workspace)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT id, external_id
            FROM conversations
            WHERE readable_id = ?
            ORDER BY COALESCE(updated_at, created_at) DESC
            LIMIT 2
            "#,
        )
        .bind(session_id)
        .fetch_all(pool)
        .await?
    };

    match readable_rows.len() {
        0 => Ok(None),
        1 => {
            let row = &readable_rows[0];
            let id: String = row.get("id");
            let external_id: Option<String> = row.get("external_id");
            Ok(Some((id, external_id)))
        }
        _ => {
            tracing::warn!(
                session_id,
                workspace = workspace.unwrap_or("<any>"),
                "Ambiguous readable_id during conversation resolution; refusing fallback"
            );
            Ok(None)
        }
    }
}
