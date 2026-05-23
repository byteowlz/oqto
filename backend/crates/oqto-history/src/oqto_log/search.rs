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
    pub content: String,
    pub score: f64,
    pub created_at: Option<String>,
}

fn build_fts_query(query: &str) -> String {
    let terms = query
        .split_whitespace()
        .map(|term| {
            let escaped = term.replace('"', "\"\"");
            format!("\"{escaped}\"")
        })
        .collect::<Vec<_>>();
    if terms.is_empty() {
        String::new()
    } else {
        terms.join(" AND ")
    }
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

    let fts_query = build_fts_query(query);
    let db_paths = resolve_db_paths(req).await?;
    let mut results = Vec::new();
    for db_path in db_paths {
        let options = SqliteConnectOptions::new().filename(&db_path);
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
              COALESCE(m.content, f.content, '') AS content,
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
        .bind(&fts_query)
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
            content: row.try_get("content").unwrap_or_default(),
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
    use std::collections::HashMap;

    use oqto_pi::AgentMessage;
    use serde_json::Value;

    use super::*;
    use crate::oqto_log::store::append_agent_end_snapshot;

    fn test_message(role: &str, content: &str) -> AgentMessage {
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

    async fn seed_session(user_home: &Path, workspace_id: &str, session_id: &str, content: &str) {
        let messages = vec![test_message("user", content)];
        append_agent_end_snapshot(
            user_home,
            "user-1",
            workspace_id,
            session_id,
            &format!("platform-{session_id}"),
            Some(&format!("external-{session_id}")),
            &format!("external-{session_id}"),
            &messages,
        )
        .await
        .expect("seed oqto-log session");
    }

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

    #[tokio::test]
    async fn searches_all_oqto_log_workspaces_without_hstry() {
        let temp = tempfile::tempdir().expect("temp home");
        seed_session(
            temp.path(),
            "/tmp/workspace-alpha",
            "alpha",
            "needle appears in alpha workspace",
        )
        .await;
        seed_session(
            temp.path(),
            "/tmp/workspace-beta",
            "beta",
            "needle appears in beta workspace",
        )
        .await;

        let response = search_timeline(&TimelineSearchRequest {
            user_home: temp.path(),
            query: "needle",
            scope: TimelineSearchScope::All,
            workspace_id: None,
            cwd: None,
            limit: 10,
        })
        .await
        .expect("search oqto-log");

        let sessions = response
            .results
            .iter()
            .map(|result| result.session_id.as_str())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(sessions.len(), 2);
        assert!(sessions.contains("alpha"));
        assert!(sessions.contains("beta"));
        assert!(response.results.iter().all(|hit| !hit.content.is_empty()));
    }

    #[tokio::test]
    async fn multi_term_search_requires_all_terms_and_escapes_quotes() {
        let temp = tempfile::tempdir().expect("temp home");
        seed_session(
            temp.path(),
            "/tmp/workspace-alpha",
            "matching",
            "flash search finds the exact regression quickly",
        )
        .await;
        seed_session(
            temp.path(),
            "/tmp/workspace-alpha",
            "nonmatching",
            "flash search misses the unrelated conversation",
        )
        .await;

        let response = search_timeline(&TimelineSearchRequest {
            user_home: temp.path(),
            query: "flash regression",
            scope: TimelineSearchScope::All,
            workspace_id: None,
            cwd: None,
            limit: 10,
        })
        .await
        .expect("multi-term search");

        assert_eq!(response.results.len(), 1);
        assert_eq!(response.results[0].session_id, "matching");

        let quoted = build_fts_query("rename \"sessions\"");
        assert_eq!(quoted, "\"rename\" AND \"\"\"sessions\"\"\"");
    }
}
