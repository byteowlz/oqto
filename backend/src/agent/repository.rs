//! Agent repository for database operations.

use anyhow::{Context, Result};
use sqlx::{FromRow, SqlitePool};
use tracing::debug;

use super::models::AgentStatus;

/// Agent record from the database.
#[derive(Debug, Clone, FromRow)]
pub struct AgentRecord {
    pub id: String,
    pub session_id: String,
    pub agent_id: String,
    pub name: String,
    pub directory: String,
    pub internal_port: i64,
    pub external_port: i64,
    #[sqlx(try_from = "String")]
    pub status: AgentStatus,
    pub has_agents_md: bool,
    pub has_git: bool,
    pub created_at: String,
    #[allow(dead_code)]
    pub started_at: Option<String>,
    #[allow(dead_code)]
    pub stopped_at: Option<String>,
}

/// Repository for agent persistence.
#[derive(Clone)]
#[allow(dead_code)]
pub struct AgentRepository {
    pool: SqlitePool,
}

#[allow(dead_code)]

impl AgentRepository {
    /// Create a new agent repository.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create a new agent record.
    pub async fn create(&self, record: &AgentRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, session_id, agent_id, name, directory,
                internal_port, external_port, status,
                has_agents_md, has_git, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&record.id)
        .bind(&record.session_id)
        .bind(&record.agent_id)
        .bind(&record.name)
        .bind(&record.directory)
        .bind(record.internal_port)
        .bind(record.external_port)
        .bind(record.status.to_string())
        .bind(record.has_agents_md)
        .bind(record.has_git)
        .bind(&record.created_at)
        .execute(&self.pool)
        .await
        .context("inserting agent record")?;

        Ok(())
    }

    /// Get an agent by its composite ID (session_id:agent_id).
    pub async fn get(&self, id: &str) -> Result<Option<AgentRecord>> {
        let record = sqlx::query_as::<_, AgentRecord>("SELECT * FROM agents WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .context("fetching agent by id")?;

        Ok(record)
    }

    /// Get an agent by session_id and agent_id.
    pub async fn get_by_session_and_agent(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> Result<Option<AgentRecord>> {
        let record = sqlx::query_as::<_, AgentRecord>(
            "SELECT * FROM agents WHERE session_id = ? AND agent_id = ?",
        )
        .bind(session_id)
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await
        .context("fetching agent by session and agent id")?;

        Ok(record)
    }

    /// List all agents for a session.
    pub async fn list_by_session(&self, session_id: &str) -> Result<Vec<AgentRecord>> {
        let records = sqlx::query_as::<_, AgentRecord>(
            "SELECT * FROM agents WHERE session_id = ? ORDER BY agent_id",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .context("listing agents by session")?;

        Ok(records)
    }

    /// List running agents for a session.
    pub async fn list_running_by_session(&self, session_id: &str) -> Result<Vec<AgentRecord>> {
        let records = sqlx::query_as::<_, AgentRecord>(
            "SELECT * FROM agents WHERE session_id = ? AND status IN ('running', 'starting') ORDER BY agent_id",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .context("listing running agents by session")?;

        Ok(records)
    }

    /// Update agent status.
    pub async fn update_status(&self, id: &str, status: AgentStatus) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();

        let (started_at, stopped_at) = match status {
            AgentStatus::Running | AgentStatus::Starting => (Some(now), None),
            AgentStatus::Stopped | AgentStatus::Failed => {
                (None, Some(chrono::Utc::now().to_rfc3339()))
            }
        };

        if let Some(started) = started_at {
            sqlx::query(
                "UPDATE agents SET status = ?, started_at = ?, stopped_at = NULL WHERE id = ?",
            )
            .bind(status.to_string())
            .bind(started)
            .bind(id)
            .execute(&self.pool)
            .await
            .context("updating agent status to running")?;
        } else if let Some(stopped) = stopped_at {
            sqlx::query("UPDATE agents SET status = ?, stopped_at = ? WHERE id = ?")
                .bind(status.to_string())
                .bind(stopped)
                .bind(id)
                .execute(&self.pool)
                .await
                .context("updating agent status to stopped")?;
        }

        Ok(())
    }

    /// Update agent metadata (has_agents_md, has_git).
    pub async fn update_metadata(
        &self,
        id: &str,
        has_agents_md: bool,
        has_git: bool,
    ) -> Result<()> {
        sqlx::query("UPDATE agents SET has_agents_md = ?, has_git = ? WHERE id = ?")
            .bind(has_agents_md)
            .bind(has_git)
            .bind(id)
            .execute(&self.pool)
            .await
            .context("updating agent metadata")?;

        Ok(())
    }

    /// Delete an agent.
    pub async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM agents WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("deleting agent")?;

        Ok(())
    }

    /// Delete all agents for a session.
    pub async fn delete_by_session(&self, session_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM agents WHERE session_id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await
            .context("deleting agents by session")?;

        Ok(())
    }

    /// Find a free external port for a new agent.
    ///
    /// Searches for an available port starting from base_port.
    /// Returns the first available port that isn't used by any running/starting agent.
    pub async fn find_free_external_port(&self, base_port: i64, max_agents: i64) -> Result<i64> {
        // Get all external ports currently in use by running/starting agents
        let used_ports: Vec<i64> = sqlx::query_scalar(
            "SELECT external_port FROM agents WHERE status IN ('running', 'starting')",
        )
        .fetch_all(&self.pool)
        .await
        .context("fetching used external ports")?;

        debug!("Used external ports: {:?}", used_ports);

        // Find the first available port
        for offset in 0..max_agents {
            let port = base_port + offset;
            if !used_ports.contains(&port) {
                return Ok(port);
            }
        }

        anyhow::bail!(
            "no available external ports for agents (checked {} ports from {})",
            max_agents,
            base_port
        )
    }

    /// Mark all agents for a session as stopped.
    ///
    /// Used when stopping a session to mark all its agents as stopped.
    pub async fn mark_all_stopped(&self, session_id: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE agents SET status = 'stopped', stopped_at = ? WHERE session_id = ? AND status IN ('running', 'starting')",
        )
        .bind(&now)
        .bind(session_id)
        .execute(&self.pool)
        .await
        .context("marking all agents as stopped")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    async fn setup() -> (Database, AgentRepository) {
        let db = Database::in_memory().await.unwrap();
        let repo = AgentRepository::new(db.pool().clone());
        (db, repo)
    }

    fn make_record(session_id: &str, agent_id: &str, external_port: i64) -> AgentRecord {
        AgentRecord {
            id: format!("{}:{}", session_id, agent_id),
            session_id: session_id.to_string(),
            agent_id: agent_id.to_string(),
            name: agent_id.to_string(),
            directory: format!("/home/dev/workspace/{}", agent_id),
            internal_port: 4001,
            external_port,
            status: AgentStatus::Stopped,
            has_agents_md: true,
            has_git: false,
            created_at: chrono::Utc::now().to_rfc3339(),
            started_at: None,
            stopped_at: None,
        }
    }

    #[tokio::test]
    async fn test_create_and_get() {
        let (_db, repo) = setup().await;

        // First create a session for the foreign key
        let session_id = "test-session";
        sqlx::query(
            "INSERT INTO sessions (id, container_name, user_id, workspace_path, image, opencode_port, fileserver_port, ttyd_port, status)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(session_id)
        .bind("test-container")
        .bind("test-user")
        .bind("/workspace")
        .bind("test-image")
        .bind(41820)
        .bind(41821)
        .bind(41822)
        .bind("running")
        .execute(&repo.pool)
        .await
        .unwrap();

        let record = make_record(session_id, "doc-writer", 41824);
        repo.create(&record).await.unwrap();

        let fetched = repo.get(&record.id).await.unwrap().unwrap();
        assert_eq!(fetched.agent_id, "doc-writer");
        assert_eq!(fetched.external_port, 41824);
    }

    #[tokio::test]
    async fn test_find_free_port() {
        let (_db, repo) = setup().await;

        // No agents, should return base port
        let port = repo.find_free_external_port(41824, 10).await.unwrap();
        assert_eq!(port, 41824);
    }
}
