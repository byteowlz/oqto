use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

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
