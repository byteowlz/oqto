use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTargetScope {
    Personal,
    SharedWorkspace,
}

impl SessionTargetScope {
    fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::SharedWorkspace => "shared_workspace",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "personal" => Some(Self::Personal),
            "shared_workspace" => Some(Self::SharedWorkspace),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionTargetRecord {
    pub session_id: String,
    pub owner_user_id: Option<String>,
    pub scope: SessionTargetScope,
    pub workspace_id: Option<String>,
    pub workspace_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionTargetRepository {
    pool: SqlitePool,
}

impl SessionTargetRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert(&self, record: &SessionTargetRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO chat_session_targets (
                session_id,
                owner_user_id,
                scope,
                workspace_id,
                workspace_path,
                updated_at
            ) VALUES (?, ?, ?, ?, ?, datetime('now'))
            ON CONFLICT(session_id) DO UPDATE SET
                owner_user_id = excluded.owner_user_id,
                scope = excluded.scope,
                workspace_id = excluded.workspace_id,
                workspace_path = excluded.workspace_path,
                updated_at = datetime('now')
            "#,
        )
        .bind(&record.session_id)
        .bind(&record.owner_user_id)
        .bind(record.scope.as_str())
        .bind(&record.workspace_id)
        .bind(&record.workspace_path)
        .execute(&self.pool)
        .await
        .context("upserting chat session target")?;

        Ok(())
    }

    pub async fn get(&self, session_id: &str) -> Result<Option<SessionTargetRecord>> {
        let row = sqlx::query_as::<_, (String, Option<String>, String, Option<String>, Option<String>)>(
            r#"
            SELECT session_id, owner_user_id, scope, workspace_id, workspace_path
            FROM chat_session_targets
            WHERE session_id = ?
            "#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .context("fetching chat session target")?;

        let Some((session_id, owner_user_id, scope, workspace_id, workspace_path)) = row else {
            return Ok(None);
        };

        let scope = SessionTargetScope::from_str(&scope)
            .ok_or_else(|| anyhow::anyhow!("invalid session target scope: {scope}"))?;

        Ok(Some(SessionTargetRecord {
            session_id,
            owner_user_id,
            scope,
            workspace_id,
            workspace_path,
        }))
    }

    pub async fn delete(&self, session_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM chat_session_targets WHERE session_id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await
            .context("deleting chat session target")?;

        Ok(())
    }
}
