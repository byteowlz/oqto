//! Session database repository.

use anyhow::{Context, Result};
use sqlx::SqlitePool;

use super::models::{Session, SessionStatus};

/// Repository for session persistence.
#[derive(Debug, Clone)]
pub struct SessionRepository {
    pool: SqlitePool,
}

impl SessionRepository {
    /// Create a new repository.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create a new session.
    pub async fn create(&self, session: &Session) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO sessions (
                id, container_id, container_name, user_id, workspace_path, image,
                opencode_port, fileserver_port, ttyd_port, eavs_port,
                eavs_key_id, eavs_key_hash, eavs_virtual_key,
                status, created_at, started_at, stopped_at, error_message
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&session.id)
        .bind(&session.container_id)
        .bind(&session.container_name)
        .bind(&session.user_id)
        .bind(&session.workspace_path)
        .bind(&session.image)
        .bind(session.opencode_port)
        .bind(session.fileserver_port)
        .bind(session.ttyd_port)
        .bind(session.eavs_port)
        .bind(&session.eavs_key_id)
        .bind(&session.eavs_key_hash)
        .bind(&session.eavs_virtual_key)
        .bind(session.status.to_string())
        .bind(&session.created_at)
        .bind(&session.started_at)
        .bind(&session.stopped_at)
        .bind(&session.error_message)
        .execute(&self.pool)
        .await
        .context("creating session")?;

        Ok(())
    }

    /// Get a session by ID.
    pub async fn get(&self, id: &str) -> Result<Option<Session>> {
        let session = sqlx::query_as::<_, Session>(
            r#"
            SELECT id, container_id, container_name, user_id, workspace_path, image,
                   opencode_port, fileserver_port, ttyd_port, eavs_port,
                   eavs_key_id, eavs_key_hash, eavs_virtual_key,
                   status, created_at, started_at, stopped_at, error_message
            FROM sessions
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("fetching session")?;

        Ok(session)
    }

    /// Get a session by container ID.
    #[allow(dead_code)]
    pub async fn get_by_container_id(&self, container_id: &str) -> Result<Option<Session>> {
        let session = sqlx::query_as::<_, Session>(
            r#"
            SELECT id, container_id, container_name, user_id, workspace_path, image,
                   opencode_port, fileserver_port, ttyd_port, eavs_port,
                   eavs_key_id, eavs_key_hash, eavs_virtual_key,
                   status, created_at, started_at, stopped_at, error_message
            FROM sessions
            WHERE container_id = ?
            "#,
        )
        .bind(container_id)
        .fetch_optional(&self.pool)
        .await
        .context("fetching session by container ID")?;

        Ok(session)
    }

    /// List all sessions.
    pub async fn list(&self) -> Result<Vec<Session>> {
        let sessions = sqlx::query_as::<_, Session>(
            r#"
            SELECT id, container_id, container_name, user_id, workspace_path, image,
                   opencode_port, fileserver_port, ttyd_port, eavs_port,
                   eavs_key_id, eavs_key_hash, eavs_virtual_key,
                   status, created_at, started_at, stopped_at, error_message
            FROM sessions
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("listing sessions")?;

        Ok(sessions)
    }

    /// List active sessions (starting or running).
    #[allow(dead_code)]
    pub async fn list_active(&self) -> Result<Vec<Session>> {
        let sessions = sqlx::query_as::<_, Session>(
            r#"
            SELECT id, container_id, container_name, user_id, workspace_path, image,
                   opencode_port, fileserver_port, ttyd_port, eavs_port,
                   eavs_key_id, eavs_key_hash, eavs_virtual_key,
                   status, created_at, started_at, stopped_at, error_message
            FROM sessions
            WHERE status IN ('pending', 'starting', 'running')
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("listing active sessions")?;

        Ok(sessions)
    }

    /// List sessions by user.
    #[allow(dead_code)]
    pub async fn list_by_user(&self, user_id: &str) -> Result<Vec<Session>> {
        let sessions = sqlx::query_as::<_, Session>(
            r#"
            SELECT id, container_id, container_name, user_id, workspace_path, image,
                   opencode_port, fileserver_port, ttyd_port, eavs_port,
                   eavs_key_id, eavs_key_hash, eavs_virtual_key,
                   status, created_at, started_at, stopped_at, error_message
            FROM sessions
            WHERE user_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .context("listing sessions by user")?;

        Ok(sessions)
    }

    /// Update session status.
    pub async fn update_status(&self, id: &str, status: SessionStatus) -> Result<()> {
        sqlx::query("UPDATE sessions SET status = ? WHERE id = ?")
            .bind(status.to_string())
            .bind(id)
            .execute(&self.pool)
            .await
            .context("updating session status")?;

        Ok(())
    }

    /// Update session with container ID and mark as starting.
    pub async fn set_container_id(&self, id: &str, container_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE sessions SET container_id = ?, status = 'starting', started_at = datetime('now') WHERE id = ?",
        )
        .bind(container_id)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("setting container ID")?;

        Ok(())
    }

    /// Mark session as running.
    pub async fn mark_running(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE sessions SET status = 'running' WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("marking session running")?;

        Ok(())
    }

    /// Mark session as stopped.
    pub async fn mark_stopped(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE sessions SET status = 'stopped', stopped_at = datetime('now') WHERE id = ?",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .context("marking session stopped")?;

        Ok(())
    }

    /// Mark session as failed with error message.
    pub async fn mark_failed(&self, id: &str, error: &str) -> Result<()> {
        sqlx::query(
            "UPDATE sessions SET status = 'failed', stopped_at = datetime('now'), error_message = ? WHERE id = ?",
        )
        .bind(error)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("marking session failed")?;

        Ok(())
    }

    /// Delete a session.
    pub async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("deleting session")?;

        Ok(())
    }

    /// Clear the EAVS virtual key from a session (for security after container starts).
    pub async fn clear_eavs_virtual_key(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE sessions SET eavs_virtual_key = NULL WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("clearing EAVS virtual key")?;

        Ok(())
    }

    /// Find a free port range starting from the given base port.
    /// Returns the first available base port (opencode_port).
    /// Now allocates 4 consecutive ports: opencode, fileserver, ttyd, eavs.
    pub async fn find_free_port_range(&self, start_port: i64) -> Result<i64> {
        // Get all ports currently in use by active sessions
        let used_ports: Vec<(i64, i64, i64, Option<i64>)> = sqlx::query_as(
            r#"
            SELECT opencode_port, fileserver_port, ttyd_port, eavs_port
            FROM sessions
            WHERE status IN ('pending', 'starting', 'running')
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("fetching used ports")?;

        let mut port = start_port;
        loop {
            let range_end = port + 4; // We need 4 consecutive ports
            let conflicts = used_ports.iter().any(|(op, fp, tp, ep)| {
                // Check if any of our ports overlap with used ports
                (port..range_end).contains(op)
                    || (port..range_end).contains(fp)
                    || (port..range_end).contains(tp)
                    || ep.map(|e| (port..range_end).contains(&e)).unwrap_or(false)
            });

            if !conflicts {
                return Ok(port);
            }

            port += 4; // Move to next potential range

            // Safety limit
            if port > 65530 {
                anyhow::bail!("no free port range available");
            }
        }
    }
}
