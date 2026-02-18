use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WorkspaceLocation {
    pub id: String,
    pub user_id: String,
    pub workspace_id: String,
    pub location_id: String,
    pub kind: String,
    pub path: String,
    pub runner_id: Option<String>,
    pub repo_fingerprint: Option<String>,
    pub is_active: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceLocationInput {
    pub id: String,
    pub user_id: String,
    pub workspace_id: String,
    pub location_id: String,
    pub kind: String,
    pub path: String,
    pub runner_id: Option<String>,
    pub repo_fingerprint: Option<String>,
    pub is_active: i64,
}

#[derive(Debug, Clone)]
pub struct WorkspaceLocationRepository {
    pool: SqlitePool,
}

impl WorkspaceLocationRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_locations(
        &self,
        user_id: &str,
        workspace_id: &str,
    ) -> Result<Vec<WorkspaceLocation>> {
        let locations = sqlx::query_as::<_, WorkspaceLocation>(
            r#"
            SELECT id, user_id, workspace_id, location_id, kind, path, runner_id, repo_fingerprint,
                   is_active, created_at, updated_at
            FROM workspace_locations
            WHERE user_id = ? AND workspace_id = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(user_id)
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
        .context("listing workspace locations")?;

        Ok(locations)
    }

    pub async fn get_active_location(
        &self,
        user_id: &str,
        workspace_id: &str,
    ) -> Result<Option<WorkspaceLocation>> {
        let location = sqlx::query_as::<_, WorkspaceLocation>(
            r#"
            SELECT id, user_id, workspace_id, location_id, kind, path, runner_id, repo_fingerprint,
                   is_active, created_at, updated_at
            FROM workspace_locations
            WHERE user_id = ? AND workspace_id = ? AND is_active = 1
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await
        .context("fetching active workspace location")?;

        Ok(location)
    }

    pub async fn get_location(
        &self,
        user_id: &str,
        workspace_id: &str,
        location_id: &str,
    ) -> Result<Option<WorkspaceLocation>> {
        let location = sqlx::query_as::<_, WorkspaceLocation>(
            r#"
            SELECT id, user_id, workspace_id, location_id, kind, path, runner_id, repo_fingerprint,
                   is_active, created_at, updated_at
            FROM workspace_locations
            WHERE user_id = ? AND workspace_id = ? AND location_id = ?
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(workspace_id)
        .bind(location_id)
        .fetch_optional(&self.pool)
        .await
        .context("fetching workspace location")?;

        Ok(location)
    }

    pub async fn upsert_location(
        &self,
        location: &WorkspaceLocationInput,
        set_active: bool,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        if set_active {
            sqlx::query(
                r#"
                UPDATE workspace_locations
                SET is_active = 0, updated_at = datetime('now')
                WHERE user_id = ? AND workspace_id = ?
                "#,
            )
            .bind(&location.user_id)
            .bind(&location.workspace_id)
            .execute(&mut *tx)
            .await
            .context("clearing active workspace location")?;
        }

        sqlx::query(
            r#"
            INSERT INTO workspace_locations (
                id, user_id, workspace_id, location_id, kind, path, runner_id, repo_fingerprint,
                is_active, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
            ON CONFLICT(user_id, workspace_id, location_id)
            DO UPDATE SET
                kind = excluded.kind,
                path = excluded.path,
                runner_id = excluded.runner_id,
                repo_fingerprint = excluded.repo_fingerprint,
                is_active = excluded.is_active,
                updated_at = datetime('now')
            "#,
        )
        .bind(&location.id)
        .bind(&location.user_id)
        .bind(&location.workspace_id)
        .bind(&location.location_id)
        .bind(&location.kind)
        .bind(&location.path)
        .bind(&location.runner_id)
        .bind(&location.repo_fingerprint)
        .bind(location.is_active)
        .execute(&mut *tx)
        .await
        .context("upserting workspace location")?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn set_active_location(
        &self,
        user_id: &str,
        workspace_id: &str,
        location_id: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            UPDATE workspace_locations
            SET is_active = 0, updated_at = datetime('now')
            WHERE user_id = ? AND workspace_id = ?
            "#,
        )
        .bind(user_id)
        .bind(workspace_id)
        .execute(&mut *tx)
        .await
        .context("clearing active workspace location")?;

        sqlx::query(
            r#"
            UPDATE workspace_locations
            SET is_active = 1, updated_at = datetime('now')
            WHERE user_id = ? AND workspace_id = ? AND location_id = ?
            "#,
        )
        .bind(user_id)
        .bind(workspace_id)
        .bind(location_id)
        .execute(&mut *tx)
        .await
        .context("setting active workspace location")?;

        tx.commit().await?;

        Ok(())
    }
}
