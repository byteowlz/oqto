//! Per-user Main Chat SQLite database.
//!
//! Each user has ONE Main Chat. The workspace lives at {workspace_dir}/main/
//! and the database tracks sessions, history, and config (including the assistant name).

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// Schema for the Main Chat database.
const SCHEMA: &str = r#"
-- History entries (summaries, decisions, handoffs, insights)
CREATE TABLE IF NOT EXISTS history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts TEXT NOT NULL,
    type TEXT NOT NULL CHECK(type IN ('summary', 'decision', 'handoff', 'insight')),
    content TEXT NOT NULL,
    session_id TEXT,
    meta TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_history_ts ON history(ts);
CREATE INDEX IF NOT EXISTS idx_history_type ON history(type);
CREATE INDEX IF NOT EXISTS idx_history_session ON history(session_id);

-- OpenCode sessions linked to this Main Chat
CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT UNIQUE NOT NULL,
    title TEXT,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    ended_at TEXT,
    message_count INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_sessions_started ON sessions(started_at);

-- Chat messages for display history (persists across Pi session restarts)
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    pi_session_id TEXT,
    timestamp INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);
CREATE INDEX IF NOT EXISTS idx_messages_pi_session ON messages(pi_session_id);

-- Key-value config store (includes assistant_name)
CREATE TABLE IF NOT EXISTS config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Schema version for future migrations
INSERT OR IGNORE INTO config (key, value) VALUES ('schema_version', '2');
"#;

/// Main Chat session title prefix - stripped in frontend display.
pub const MAIN_CHAT_TITLE_PREFIX: &str = "[[main]]";

/// Per-user Main Chat database connection.
#[derive(Debug, Clone)]
pub struct MainChatDb {
    pool: SqlitePool,
    path: PathBuf,
}

impl MainChatDb {
    /// Open or create the Main Chat database.
    ///
    /// Creates the database file and parent directories if they don't exist.
    pub async fn open(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating main chat directory: {}", parent.display()))?;
        }

        let database_url = format!("sqlite://{}?mode=rwc", path.display());

        let options = SqliteConnectOptions::from_str(&database_url)
            .context("parsing database URL")?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(30));

        let pool = SqlitePoolOptions::new()
            .max_connections(3)
            .connect_with(options)
            .await
            .with_context(|| format!("connecting to main chat database: {}", path.display()))?;

        let db = Self {
            pool,
            path: path.to_path_buf(),
        };
        db.initialize_schema().await?;

        Ok(db)
    }

    /// Initialize the database schema.
    async fn initialize_schema(&self) -> Result<()> {
        sqlx::raw_sql(SCHEMA)
            .execute(&self.pool)
            .await
            .context("initializing main chat database schema")?;
        Ok(())
    }

    /// Get a reference to the connection pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Get the database file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Close the database connection.
    pub async fn close(&self) {
        self.pool.close().await;
    }

    /// Check if the database is healthy.
    pub async fn is_healthy(&self) -> bool {
        sqlx::query("SELECT 1").fetch_one(&self.pool).await.is_ok()
    }
}

/// Get the path to a user's Main Chat directory.
///
/// For single-user mode: `{workspace_dir}/main/`
/// For multi-user mode: `{workspace_dir}/{user_id}/main/`
pub fn main_chat_dir_path(workspace_dir: &Path, user_id: &str, single_user: bool) -> PathBuf {
    if single_user {
        workspace_dir.join("main")
    } else {
        workspace_dir.join(user_id).join("main")
    }
}

/// Get the path to a user's Main Chat database.
///
/// Returns: `{main_chat_dir}/main_chat.db`
pub fn main_chat_db_path(workspace_dir: &Path, user_id: &str, single_user: bool) -> PathBuf {
    main_chat_dir_path(workspace_dir, user_id, single_user).join("main_chat.db")
}

/// Check if a user has a Main Chat set up.
pub fn main_chat_exists(workspace_dir: &Path, user_id: &str, single_user: bool) -> bool {
    main_chat_db_path(workspace_dir, user_id, single_user).exists()
}

/// Create a session title with the Main Chat prefix.
pub fn prefixed_title(title: &str) -> String {
    format!("{} {}", MAIN_CHAT_TITLE_PREFIX, title)
}

/// Strip the Main Chat prefix from a session title.
pub fn strip_title_prefix(title: &str) -> &str {
    title
        .strip_prefix(MAIN_CHAT_TITLE_PREFIX)
        .map(|s| s.trim_start())
        .unwrap_or(title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_create_and_open() {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");

        let db = MainChatDb::open(&db_path).await.unwrap();
        assert!(db.is_healthy().await);
        assert!(db_path.exists());

        db.close().await;
    }

    #[tokio::test]
    async fn test_main_chat_paths_single_user() {
        let workspace_dir = Path::new("/home/user/octo");

        let dir = main_chat_dir_path(workspace_dir, "ignored", true);
        assert_eq!(dir, PathBuf::from("/home/user/octo/main"));

        let db = main_chat_db_path(workspace_dir, "ignored", true);
        assert_eq!(db, PathBuf::from("/home/user/octo/main/main_chat.db"));
    }

    #[tokio::test]
    async fn test_main_chat_paths_multi_user() {
        let workspace_dir = Path::new("/data/octo/workspaces");

        let dir = main_chat_dir_path(workspace_dir, "user123", false);
        assert_eq!(dir, PathBuf::from("/data/octo/workspaces/user123/main"));

        let db = main_chat_db_path(workspace_dir, "user123", false);
        assert_eq!(
            db,
            PathBuf::from("/data/octo/workspaces/user123/main/main_chat.db")
        );
    }

    #[test]
    fn test_title_prefix() {
        let title = "2025-01-04";
        let prefixed = prefixed_title(title);
        assert_eq!(prefixed, "[[main]] 2025-01-04");

        let stripped = strip_title_prefix(&prefixed);
        assert_eq!(stripped, "2025-01-04");

        // Non-prefixed titles are returned as-is
        let normal = "Regular session";
        assert_eq!(strip_title_prefix(normal), "Regular session");
    }
}
