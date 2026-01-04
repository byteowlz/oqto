//! Repository for main chat database operations.

use anyhow::{Context, Result};
use chrono::Utc;

use super::db::MainChatDb;
use super::models::{
    AssistantConfig, ChatMessage, CreateChatMessage, CreateHistoryEntry, CreateSession,
    HistoryEntry, MainChatSession,
};

/// Repository for main chat operations.
pub struct MainChatRepository<'a> {
    db: &'a MainChatDb,
}

impl<'a> MainChatRepository<'a> {
    /// Create a new repository instance.
    pub fn new(db: &'a MainChatDb) -> Self {
        Self { db }
    }

    // ========== History Operations ==========

    /// Add a history entry.
    pub async fn add_history(&self, entry: CreateHistoryEntry) -> Result<HistoryEntry> {
        let ts = Utc::now().to_rfc3339();
        let entry_type = entry.entry_type.to_string();
        let meta = entry.meta.map(|m| m.to_string());

        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO history (ts, type, content, session_id, meta)
            VALUES (?, ?, ?, ?, ?)
            RETURNING id
            "#,
        )
        .bind(&ts)
        .bind(&entry_type)
        .bind(&entry.content)
        .bind(&entry.session_id)
        .bind(&meta)
        .fetch_one(self.db.pool())
        .await
        .context("inserting history entry")?;

        self.get_history_by_id(id).await
    }

    /// Get a history entry by ID.
    pub async fn get_history_by_id(&self, id: i64) -> Result<HistoryEntry> {
        sqlx::query_as::<_, HistoryEntry>(
            "SELECT id, ts, type, content, session_id, meta, created_at FROM history WHERE id = ?",
        )
        .bind(id)
        .fetch_one(self.db.pool())
        .await
        .context("fetching history entry")
    }

    /// Get recent history entries.
    pub async fn get_recent_history(&self, limit: i64) -> Result<Vec<HistoryEntry>> {
        sqlx::query_as::<_, HistoryEntry>(
            r#"
            SELECT id, ts, type, content, session_id, meta, created_at
            FROM history
            ORDER BY ts DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(self.db.pool())
        .await
        .context("fetching recent history")
    }

    /// Get history entries by type.
    pub async fn get_history_by_type(
        &self,
        entry_type: &str,
        limit: i64,
    ) -> Result<Vec<HistoryEntry>> {
        sqlx::query_as::<_, HistoryEntry>(
            r#"
            SELECT id, ts, type, content, session_id, meta, created_at
            FROM history
            WHERE type = ?
            ORDER BY ts DESC
            LIMIT ?
            "#,
        )
        .bind(entry_type)
        .bind(limit)
        .fetch_all(self.db.pool())
        .await
        .context("fetching history by type")
    }

    /// Get history entries for a session.
    pub async fn get_history_for_session(&self, session_id: &str) -> Result<Vec<HistoryEntry>> {
        sqlx::query_as::<_, HistoryEntry>(
            r#"
            SELECT id, ts, type, content, session_id, meta, created_at
            FROM history
            WHERE session_id = ?
            ORDER BY ts ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(self.db.pool())
        .await
        .context("fetching history for session")
    }

    /// Count total history entries.
    pub async fn count_history(&self) -> Result<i64> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM history")
            .fetch_one(self.db.pool())
            .await
            .context("counting history entries")
    }

    /// Delete old history entries, keeping the most recent N.
    pub async fn prune_history(&self, keep_count: i64) -> Result<i64> {
        let result = sqlx::query(
            r#"
            DELETE FROM history
            WHERE id NOT IN (
                SELECT id FROM history ORDER BY ts DESC LIMIT ?
            )
            "#,
        )
        .bind(keep_count)
        .execute(self.db.pool())
        .await
        .context("pruning history")?;

        Ok(result.rows_affected() as i64)
    }

    // ========== Session Operations ==========

    /// Register a new session.
    pub async fn add_session(&self, session: CreateSession) -> Result<MainChatSession> {
        let started_at = Utc::now().to_rfc3339();

        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO sessions (session_id, title, started_at)
            VALUES (?, ?, ?)
            ON CONFLICT(session_id) DO UPDATE SET title = excluded.title
            RETURNING id
            "#,
        )
        .bind(&session.session_id)
        .bind(&session.title)
        .bind(&started_at)
        .fetch_one(self.db.pool())
        .await
        .context("inserting session")?;

        self.get_session_by_id(id).await
    }

    /// Get a session by ID.
    pub async fn get_session_by_id(&self, id: i64) -> Result<MainChatSession> {
        sqlx::query_as::<_, MainChatSession>(
            "SELECT id, session_id, title, started_at, ended_at, message_count FROM sessions WHERE id = ?",
        )
        .bind(id)
        .fetch_one(self.db.pool())
        .await
        .context("fetching session")
    }

    /// Get a session by OpenCode session ID.
    pub async fn get_session_by_session_id(
        &self,
        session_id: &str,
    ) -> Result<Option<MainChatSession>> {
        sqlx::query_as::<_, MainChatSession>(
            "SELECT id, session_id, title, started_at, ended_at, message_count FROM sessions WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(self.db.pool())
        .await
        .context("fetching session by session_id")
    }

    /// List all sessions.
    pub async fn list_sessions(&self) -> Result<Vec<MainChatSession>> {
        sqlx::query_as::<_, MainChatSession>(
            "SELECT id, session_id, title, started_at, ended_at, message_count FROM sessions ORDER BY started_at DESC",
        )
        .fetch_all(self.db.pool())
        .await
        .context("listing sessions")
    }

    /// Get the most recent session.
    pub async fn get_latest_session(&self) -> Result<Option<MainChatSession>> {
        sqlx::query_as::<_, MainChatSession>(
            "SELECT id, session_id, title, started_at, ended_at, message_count FROM sessions ORDER BY started_at DESC LIMIT 1",
        )
        .fetch_optional(self.db.pool())
        .await
        .context("fetching latest session")
    }

    /// Update session message count.
    pub async fn update_session_message_count(&self, session_id: &str, count: i64) -> Result<()> {
        sqlx::query("UPDATE sessions SET message_count = ? WHERE session_id = ?")
            .bind(count)
            .bind(session_id)
            .execute(self.db.pool())
            .await
            .context("updating session message count")?;
        Ok(())
    }

    /// Mark a session as ended.
    pub async fn end_session(&self, session_id: &str) -> Result<()> {
        let ended_at = Utc::now().to_rfc3339();
        sqlx::query("UPDATE sessions SET ended_at = ? WHERE session_id = ?")
            .bind(&ended_at)
            .bind(session_id)
            .execute(self.db.pool())
            .await
            .context("ending session")?;
        Ok(())
    }

    /// Count total sessions.
    pub async fn count_sessions(&self) -> Result<i64> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions")
            .fetch_one(self.db.pool())
            .await
            .context("counting sessions")
    }

    // ========== Config Operations ==========

    /// Get a config value.
    pub async fn get_config(&self, key: &str) -> Result<Option<String>> {
        sqlx::query_scalar::<_, String>("SELECT value FROM config WHERE key = ?")
            .bind(key)
            .fetch_optional(self.db.pool())
            .await
            .context("fetching config")
    }

    /// Set a config value.
    pub async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO config (key, value) VALUES (?, ?)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
        )
        .bind(key)
        .bind(value)
        .execute(self.db.pool())
        .await
        .context("setting config")?;
        Ok(())
    }

    /// Get all config values.
    pub async fn get_all_config(&self) -> Result<Vec<AssistantConfig>> {
        sqlx::query_as::<_, AssistantConfig>("SELECT key, value FROM config")
            .fetch_all(self.db.pool())
            .await
            .context("fetching all config")
    }

    /// Delete a config value.
    pub async fn delete_config(&self, key: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM config WHERE key = ?")
            .bind(key)
            .execute(self.db.pool())
            .await
            .context("deleting config")?;
        Ok(result.rows_affected() > 0)
    }

    // ========== Message Operations ==========

    /// Add a chat message.
    pub async fn add_message(&self, message: CreateChatMessage) -> Result<ChatMessage> {
        let timestamp = Utc::now().timestamp_millis();
        let role = message.role.to_string();
        let content = message.content.to_string();

        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO messages (role, content, pi_session_id, timestamp)
            VALUES (?, ?, ?, ?)
            RETURNING id
            "#,
        )
        .bind(&role)
        .bind(&content)
        .bind(&message.pi_session_id)
        .bind(timestamp)
        .fetch_one(self.db.pool())
        .await
        .context("inserting message")?;

        self.get_message_by_id(id).await
    }

    /// Get a message by ID.
    pub async fn get_message_by_id(&self, id: i64) -> Result<ChatMessage> {
        sqlx::query_as::<_, ChatMessage>(
            "SELECT id, role, content, pi_session_id, timestamp, created_at FROM messages WHERE id = ?",
        )
        .bind(id)
        .fetch_one(self.db.pool())
        .await
        .context("fetching message")
    }

    /// Get all messages (for display history).
    pub async fn get_all_messages(&self) -> Result<Vec<ChatMessage>> {
        sqlx::query_as::<_, ChatMessage>(
            r#"
            SELECT id, role, content, pi_session_id, timestamp, created_at
            FROM messages
            ORDER BY timestamp ASC
            "#,
        )
        .fetch_all(self.db.pool())
        .await
        .context("fetching all messages")
    }

    /// Get recent messages with limit.
    pub async fn get_recent_messages(&self, limit: i64) -> Result<Vec<ChatMessage>> {
        sqlx::query_as::<_, ChatMessage>(
            r#"
            SELECT id, role, content, pi_session_id, timestamp, created_at
            FROM messages
            ORDER BY timestamp DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(self.db.pool())
        .await
        .context("fetching recent messages")
    }

    /// Get messages since a timestamp.
    pub async fn get_messages_since(&self, since_timestamp: i64) -> Result<Vec<ChatMessage>> {
        sqlx::query_as::<_, ChatMessage>(
            r#"
            SELECT id, role, content, pi_session_id, timestamp, created_at
            FROM messages
            WHERE timestamp > ?
            ORDER BY timestamp ASC
            "#,
        )
        .bind(since_timestamp)
        .fetch_all(self.db.pool())
        .await
        .context("fetching messages since timestamp")
    }

    /// Get messages for a specific Pi session.
    pub async fn get_messages_for_pi_session(
        &self,
        pi_session_id: &str,
    ) -> Result<Vec<ChatMessage>> {
        sqlx::query_as::<_, ChatMessage>(
            r#"
            SELECT id, role, content, pi_session_id, timestamp, created_at
            FROM messages
            WHERE pi_session_id = ?
            ORDER BY timestamp ASC
            "#,
        )
        .bind(pi_session_id)
        .fetch_all(self.db.pool())
        .await
        .context("fetching messages for Pi session")
    }

    /// Count total messages.
    pub async fn count_messages(&self) -> Result<i64> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM messages")
            .fetch_one(self.db.pool())
            .await
            .context("counting messages")
    }

    /// Delete old messages, keeping the most recent N.
    pub async fn prune_messages(&self, keep_count: i64) -> Result<i64> {
        let result = sqlx::query(
            r#"
            DELETE FROM messages
            WHERE id NOT IN (
                SELECT id FROM messages ORDER BY timestamp DESC LIMIT ?
            )
            "#,
        )
        .bind(keep_count)
        .execute(self.db.pool())
        .await
        .context("pruning messages")?;

        Ok(result.rows_affected() as i64)
    }

    /// Delete all messages (for new session with fresh history).
    pub async fn clear_messages(&self) -> Result<i64> {
        let result = sqlx::query("DELETE FROM messages")
            .execute(self.db.pool())
            .await
            .context("clearing messages")?;

        Ok(result.rows_affected() as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::main_chat::models::HistoryEntryType;
    use tempfile::TempDir;

    async fn setup() -> (TempDir, MainChatDb) {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");
        let db = MainChatDb::open(&db_path).await.unwrap();
        (temp, db)
    }

    #[tokio::test]
    async fn test_history_crud() {
        let (_temp, db) = setup().await;
        let repo = MainChatRepository::new(&db);

        // Create
        let entry = repo
            .add_history(CreateHistoryEntry {
                entry_type: HistoryEntryType::Decision,
                content: "Chose SQLite for storage".to_string(),
                session_id: Some("sess-123".to_string()),
                meta: None,
            })
            .await
            .unwrap();

        assert_eq!(entry.content, "Chose SQLite for storage");
        assert_eq!(entry.entry_type, "decision");

        // Read
        let fetched = repo.get_history_by_id(entry.id).await.unwrap();
        assert_eq!(fetched.id, entry.id);

        // List
        let recent = repo.get_recent_history(10).await.unwrap();
        assert_eq!(recent.len(), 1);

        // Count
        let count = repo.count_history().await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_session_crud() {
        let (_temp, db) = setup().await;
        let repo = MainChatRepository::new(&db);

        // Create
        let session = repo
            .add_session(CreateSession {
                session_id: "oc-12345".to_string(),
                title: Some("Test session".to_string()),
            })
            .await
            .unwrap();

        assert_eq!(session.session_id, "oc-12345");

        // Read
        let fetched = repo
            .get_session_by_session_id("oc-12345")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.id, session.id);

        // List
        let sessions = repo.list_sessions().await.unwrap();
        assert_eq!(sessions.len(), 1);

        // Update count
        repo.update_session_message_count("oc-12345", 42)
            .await
            .unwrap();
        let updated = repo
            .get_session_by_session_id("oc-12345")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.message_count, 42);
    }

    #[tokio::test]
    async fn test_config_crud() {
        let (_temp, db) = setup().await;
        let repo = MainChatRepository::new(&db);

        // Set
        repo.set_config("theme", "dark").await.unwrap();

        // Get
        let value = repo.get_config("theme").await.unwrap();
        assert_eq!(value, Some("dark".to_string()));

        // Update
        repo.set_config("theme", "light").await.unwrap();
        let value = repo.get_config("theme").await.unwrap();
        assert_eq!(value, Some("light".to_string()));

        // Delete
        let deleted = repo.delete_config("theme").await.unwrap();
        assert!(deleted);
        let value = repo.get_config("theme").await.unwrap();
        assert!(value.is_none());
    }
}
