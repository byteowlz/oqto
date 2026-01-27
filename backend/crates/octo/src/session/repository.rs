//! Session database repository.

use anyhow::{Context, Result};
use sqlx::SqlitePool;

use super::models::{Session, SessionStatus};

/// All session columns for SELECT queries.
const SESSION_COLUMNS: &str = r#"
    id, readable_id, container_id, container_name, user_id, workspace_path, agent, image, image_digest,
    opencode_port, fileserver_port, ttyd_port, eavs_port, agent_base_port, max_agents,
    eavs_key_id, eavs_key_hash, eavs_virtual_key, mmry_port,
    status, runtime_mode, created_at, started_at, stopped_at, last_activity_at, error_message
"#;

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
                id, readable_id, container_id, container_name, user_id, workspace_path, agent, image, image_digest,
                opencode_port, fileserver_port, ttyd_port, eavs_port, agent_base_port, max_agents,
                eavs_key_id, eavs_key_hash, eavs_virtual_key, mmry_port,
                status, runtime_mode, created_at, started_at, stopped_at, last_activity_at, error_message
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&session.id)
        .bind(&session.readable_id)
        .bind(&session.container_id)
        .bind(&session.container_name)
        .bind(&session.user_id)
        .bind(&session.workspace_path)
        .bind(&session.agent)
        .bind(&session.image)
        .bind(&session.image_digest)
        .bind(session.opencode_port)
        .bind(session.fileserver_port)
        .bind(session.ttyd_port)
        .bind(session.eavs_port)
        .bind(session.agent_base_port)
        .bind(session.max_agents)
        .bind(&session.eavs_key_id)
        .bind(&session.eavs_key_hash)
        .bind(&session.eavs_virtual_key)
        .bind(session.mmry_port)
        .bind(session.status.to_string())
        .bind(session.runtime_mode.to_string())
        .bind(&session.created_at)
        .bind(&session.started_at)
        .bind(&session.stopped_at)
        .bind(&session.last_activity_at)
        .bind(&session.error_message)
        .execute(&self.pool)
        .await
        .context("creating session")?;

        Ok(())
    }

    /// Update EAVS key metadata for a session.
    pub async fn update_eavs_keys(
        &self,
        session_id: &str,
        key_id: Option<&str>,
        key_hash: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE sessions
            SET eavs_key_id = ?, eavs_key_hash = ?
            WHERE id = ?
            "#,
        )
        .bind(key_id)
        .bind(key_hash)
        .bind(session_id)
        .execute(&self.pool)
        .await
        .context("updating session eavs keys")?;

        Ok(())
    }

    /// Get a session by ID.
    pub async fn get(&self, id: &str) -> Result<Option<Session>> {
        let query = format!("SELECT {} FROM sessions WHERE id = ?", SESSION_COLUMNS);
        let session = sqlx::query_as::<_, Session>(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .context("fetching session")?;

        Ok(session)
    }

    /// Get a session by container ID.
    #[allow(dead_code)]
    pub async fn get_by_container_id(&self, container_id: &str) -> Result<Option<Session>> {
        let query = format!(
            "SELECT {} FROM sessions WHERE container_id = ?",
            SESSION_COLUMNS
        );
        let session = sqlx::query_as::<_, Session>(&query)
            .bind(container_id)
            .fetch_optional(&self.pool)
            .await
            .context("fetching session by container ID")?;

        Ok(session)
    }

    /// List all sessions.
    pub async fn list(&self) -> Result<Vec<Session>> {
        let query = format!(
            "SELECT {} FROM sessions ORDER BY created_at DESC",
            SESSION_COLUMNS
        );
        let sessions = sqlx::query_as::<_, Session>(&query)
            .fetch_all(&self.pool)
            .await
            .context("listing sessions")?;

        Ok(sessions)
    }

    /// List active sessions (starting or running).
    #[allow(dead_code)]
    pub async fn list_active(&self) -> Result<Vec<Session>> {
        let query = format!(
            "SELECT {} FROM sessions WHERE status IN ('pending', 'starting', 'running') ORDER BY created_at DESC",
            SESSION_COLUMNS
        );
        let sessions = sqlx::query_as::<_, Session>(&query)
            .fetch_all(&self.pool)
            .await
            .context("listing active sessions")?;

        Ok(sessions)
    }

    /// List running sessions for a user.
    pub async fn list_running_for_user(&self, user_id: &str) -> Result<Vec<Session>> {
        let query = format!(
            "SELECT {} FROM sessions WHERE user_id = ? AND status = 'running' ORDER BY created_at DESC",
            SESSION_COLUMNS
        );
        let sessions = sqlx::query_as::<_, Session>(&query)
            .bind(user_id)
            .fetch_all(&self.pool)
            .await
            .context("listing running sessions for user")?;

        Ok(sessions)
    }

    pub async fn list_for_user(&self, user_id: &str) -> Result<Vec<Session>> {
        let sessions = sqlx::query_as::<_, Session>(
            r#"
            SELECT * FROM sessions
            WHERE user_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .context("listing sessions for user")?;

        Ok(sessions)
    }

    /// List sessions by user.
    #[allow(dead_code)]
    pub async fn list_by_user(&self, user_id: &str) -> Result<Vec<Session>> {
        let query = format!(
            "SELECT {} FROM sessions WHERE user_id = ? ORDER BY created_at DESC",
            SESSION_COLUMNS
        );
        let sessions = sqlx::query_as::<_, Session>(&query)
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

    /// Update session ports after a local resume reassigns them.
    pub async fn update_ports(
        &self,
        id: &str,
        opencode_port: i64,
        fileserver_port: i64,
        ttyd_port: i64,
        mmry_port: Option<i64>,
        agent_base_port: Option<i64>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE sessions SET opencode_port = ?, fileserver_port = ?, ttyd_port = ?, mmry_port = ?, agent_base_port = ? WHERE id = ?",
        )
        .bind(opencode_port)
        .bind(fileserver_port)
        .bind(ttyd_port)
        .bind(mmry_port)
        .bind(agent_base_port)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("updating session ports")?;

        Ok(())
    }

    /// Set the mmry port for a session.
    pub async fn set_mmry_port(&self, id: &str, mmry_port: Option<i64>) -> Result<()> {
        sqlx::query("UPDATE sessions SET mmry_port = ? WHERE id = ?")
            .bind(mmry_port)
            .bind(id)
            .execute(&self.pool)
            .await
            .context("setting mmry port")?;

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

    /// Find a stopped session for a user that can be resumed.
    ///
    /// Returns the most recently stopped session for the user that still has a container/PIDs.
    pub async fn find_resumable_session(&self, user_id: &str) -> Result<Option<Session>> {
        let query = format!(
            "SELECT {} FROM sessions WHERE user_id = ? AND status = 'stopped' AND container_id IS NOT NULL ORDER BY stopped_at DESC LIMIT 1",
            SESSION_COLUMNS
        );
        let session = sqlx::query_as::<_, Session>(&query)
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .context("finding resumable session")?;

        Ok(session)
    }

    /// List stopped sessions that have been stopped for longer than the given duration.
    ///
    /// Used for cleanup of old stopped containers.
    pub async fn list_stale_stopped_sessions(&self, older_than_hours: i64) -> Result<Vec<Session>> {
        let query = format!(
            "SELECT {} FROM sessions WHERE status = 'stopped' AND container_id IS NOT NULL AND stopped_at < datetime('now', ? || ' hours') ORDER BY stopped_at ASC",
            SESSION_COLUMNS
        );
        let sessions = sqlx::query_as::<_, Session>(&query)
            .bind(-older_than_hours) // negative for "X hours ago"
            .fetch_all(&self.pool)
            .await
            .context("listing stale stopped sessions")?;

        Ok(sessions)
    }

    /// Update the image digest for a session.
    pub async fn update_image_digest(&self, id: &str, digest: &str) -> Result<()> {
        sqlx::query("UPDATE sessions SET image_digest = ? WHERE id = ?")
            .bind(digest)
            .bind(id)
            .execute(&self.pool)
            .await
            .context("updating image digest")?;

        Ok(())
    }

    /// Update image and digest for a session (used during upgrade).
    pub async fn update_image_and_digest(
        &self,
        id: &str,
        image: &str,
        digest: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE sessions SET image = ?, image_digest = ? WHERE id = ?")
            .bind(image)
            .bind(digest)
            .bind(id)
            .execute(&self.pool)
            .await
            .context("updating image and digest")?;

        Ok(())
    }

    /// Clear container ID (used when recreating container during upgrade).
    pub async fn clear_container_id(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE sessions SET container_id = NULL WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("clearing container ID")?;

        Ok(())
    }

    /// Update last activity timestamp for a session.
    pub async fn touch_activity(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE sessions SET last_activity_at = datetime('now') WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("updating last activity")?;

        Ok(())
    }

    /// List running sessions that have been idle for longer than the given duration.
    pub async fn list_idle_sessions(&self, idle_minutes: i64) -> Result<Vec<Session>> {
        let query = format!(
            "SELECT {} FROM sessions WHERE status = 'running' AND (last_activity_at IS NULL OR datetime(last_activity_at) < datetime('now', ? || ' minutes')) ORDER BY datetime(last_activity_at) ASC",
            SESSION_COLUMNS
        );
        let sessions = sqlx::query_as::<_, Session>(&query)
            .bind(-idle_minutes) // negative for "X minutes ago"
            .fetch_all(&self.pool)
            .await
            .context("listing idle sessions")?;

        Ok(sessions)
    }

    /// Find a running session for a specific workspace path.
    pub async fn find_running_for_workspace(
        &self,
        user_id: &str,
        workspace_path: &str,
    ) -> Result<Option<Session>> {
        let query = format!(
            "SELECT {} FROM sessions WHERE user_id = ? AND workspace_path = ? AND status = 'running' LIMIT 1",
            SESSION_COLUMNS
        );
        let session = sqlx::query_as::<_, Session>(&query)
            .bind(user_id)
            .bind(workspace_path)
            .fetch_optional(&self.pool)
            .await
            .context("finding running session for workspace")?;

        Ok(session)
    }

    /// Find the most recently stopped session for a specific workspace path.
    pub async fn find_latest_stopped_for_workspace(
        &self,
        user_id: &str,
        workspace_path: &str,
    ) -> Result<Option<Session>> {
        let query = format!(
            "SELECT {} FROM sessions WHERE user_id = ? AND workspace_path = ? AND status = 'stopped' AND container_id IS NOT NULL ORDER BY stopped_at DESC LIMIT 1",
            SESSION_COLUMNS
        );
        let session = sqlx::query_as::<_, Session>(&query)
            .bind(user_id)
            .bind(workspace_path)
            .fetch_optional(&self.pool)
            .await
            .context("finding stopped session for workspace")?;

        Ok(session)
    }

    /// Count running sessions for a user.
    pub async fn count_running_for_user(&self, user_id: &str) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sessions WHERE user_id = ? AND status = 'running'",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .context("counting running sessions")?;

        Ok(count.0)
    }

    /// Find a free port range with a specific number of agent ports.
    /// Returns the first available base port (opencode_port).
    /// Allocates: opencode (base), fileserver (base+1), ttyd (base+2), agents (base+3 to base+3+agent_count-1).
    pub async fn find_free_port_range_with_agents(
        &self,
        start_port: i64,
        agent_count: i64,
    ) -> Result<i64> {
        // Get all port ranges currently in use by active sessions
        let used_ranges: Vec<(i64, i64, i64, Option<i64>, Option<i64>)> = sqlx::query_as(
            r#"
            SELECT opencode_port, fileserver_port, ttyd_port, agent_base_port, max_agents
            FROM sessions
            WHERE status IN ('pending', 'starting', 'running')
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("fetching used ports")?;

        // We need: opencode, fileserver, ttyd (3 ports) + agent_count ports for sub-agents
        let ports_needed = 3 + agent_count;
        let mut port = start_port;

        loop {
            let range_end = port + ports_needed;

            let conflicts = used_ranges.iter().any(|(op, fp, tp, abp, ma)| {
                // Check if any of our ports overlap with used session ports
                let session_ports_conflict = (port..range_end).contains(op)
                    || (port..range_end).contains(fp)
                    || (port..range_end).contains(tp);

                // Check if we overlap with their agent port range
                let agent_range_conflict = if let (Some(agent_base), Some(max)) = (abp, ma) {
                    let their_agent_end = agent_base + max;
                    // Check for overlap between [port, range_end) and [agent_base, their_agent_end)
                    port < their_agent_end && range_end > *agent_base
                } else {
                    false
                };

                session_ports_conflict || agent_range_conflict
            });

            if !conflicts {
                return Ok(port);
            }

            port += ports_needed; // Move to next potential range

            // Safety limit
            if port > 65530 {
                anyhow::bail!("no free port range available");
            }
        }
    }
}
