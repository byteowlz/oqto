use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineSearchScope {
    Workdir,
    Workspace,
    All,
}

#[derive(Debug, Clone)]
pub struct TimelineSearchRequest<'a> {
    pub user_home: &'a Path,
    pub query: &'a str,
    pub scope: TimelineSearchScope,
    pub workspace_id: Option<&'a str>,
    pub cwd: Option<&'a Path>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineSearchResponse {
    pub schema_version: u32,
    pub query: String,
    pub scope: TimelineSearchScope,
    pub results: Vec<TimelineSearchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineSearchResult {
    pub session_id: String,
    pub platform_id: String,
    pub external_id: Option<String>,
    pub workspace_id: Option<String>,
    pub branch_id: String,
    pub turn_id: String,
    pub message_id: String,
    pub role: String,
    pub snippet: String,
    pub score: f64,
    pub created_at: Option<String>,
}

pub async fn search_timeline(req: &TimelineSearchRequest<'_>) -> Result<TimelineSearchResponse> {
    let query = req.query.trim();
    if query.is_empty() {
        return Ok(TimelineSearchResponse {
            schema_version: 1,
            query: String::new(),
            scope: req.scope,
            results: Vec::new(),
        });
    }

    let db_paths = resolve_db_paths(req).await?;
    let mut results = Vec::new();
    for db_path in db_paths {
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
            SELECT
              s.session_id AS session_id,
              s.platform_id AS platform_id,
              s.external_id AS external_id,
              s.workspace_id AS workspace_id,
              t.branch_id AS branch_id,
              f.turn_id AS turn_id,
              f.message_id AS message_id,
              f.role AS role,
              snippet(oqto_log_message_fts, 4, '[', ']', '…', 12) AS snippet,
              bm25(oqto_log_message_fts) AS score,
              m.created_at AS created_at
            FROM oqto_log_message_fts f
            JOIN oqto_log_turns t ON t.turn_id = f.turn_id
            JOIN oqto_log_sessions s ON s.session_id = t.session_id
            LEFT JOIN oqto_log_messages m ON m.message_id = f.message_id
            WHERE oqto_log_message_fts MATCH ?
            ORDER BY score ASC
            LIMIT ?
            "#,
        )
        .bind(query)
        .bind(req.limit.max(1) as i64)
        .fetch_all(&pool)
        .await
        .with_context(|| format!("search oqto-log db: {}", db_path.display()))?;

        results.extend(rows.into_iter().map(|row| TimelineSearchResult {
            session_id: row.try_get("session_id").unwrap_or_default(),
            platform_id: row.try_get("platform_id").unwrap_or_default(),
            external_id: row.try_get("external_id").ok(),
            workspace_id: row.try_get("workspace_id").ok(),
            branch_id: row.try_get("branch_id").unwrap_or_default(),
            turn_id: row.try_get("turn_id").unwrap_or_default(),
            message_id: row.try_get("message_id").unwrap_or_default(),
            role: row.try_get("role").unwrap_or_default(),
            snippet: row.try_get("snippet").unwrap_or_default(),
            score: row.try_get("score").unwrap_or(0.0),
            created_at: row.try_get("created_at").ok(),
        }));
    }

    results.sort_by(|a, b| a.score.total_cmp(&b.score));
    results.truncate(req.limit.max(1));
    Ok(TimelineSearchResponse {
        schema_version: 1,
        query: query.to_string(),
        scope: req.scope,
        results,
    })
}

async fn resolve_db_paths(req: &TimelineSearchRequest<'_>) -> Result<Vec<PathBuf>> {
    match req.scope {
        TimelineSearchScope::Workspace => {
            let workspace_id = req
                .workspace_id
                .context("--workspace-id is required for workspace scope")?;
            Ok(vec![
                crate::oqto_log::paths::resolve_user_home_workspace_db_path(
                    req.user_home,
                    workspace_id,
                )?,
            ])
        }
        TimelineSearchScope::Workdir => {
            if let Some(workspace_id) = req.workspace_id {
                return Ok(vec![
                    crate::oqto_log::paths::resolve_user_home_workspace_db_path(
                        req.user_home,
                        workspace_id,
                    )?,
                ]);
            }
            let cwd = req.cwd.context("cwd is required for workdir scope")?;
            let workspace_id = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
            let db = crate::oqto_log::paths::resolve_user_home_workspace_db_path(
                req.user_home,
                &workspace_id.to_string_lossy(),
            )?;
            Ok(if db.exists() { vec![db] } else { Vec::new() })
        }
        TimelineSearchScope::All => Ok(list_existing_db_paths(req.user_home).await),
    }
}

async fn list_existing_db_paths(user_home: &Path) -> Vec<PathBuf> {
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
        let path = entry.path().join("oqto-log.sqlite");
        if path.exists() {
            out.push(path);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_query_returns_empty_response() {
        let response = search_timeline(&TimelineSearchRequest {
            user_home: Path::new("/tmp/no-such-user-home"),
            query: " ",
            scope: TimelineSearchScope::All,
            workspace_id: None,
            cwd: None,
            limit: 10,
        })
        .await
        .expect("empty search");
        assert_eq!(response.schema_version, 1);
        assert!(response.results.is_empty());
    }
}
