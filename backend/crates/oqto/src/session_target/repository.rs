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

    fn is_shared_workspace_path(path: &str) -> bool {
        path.starts_with("/home/oqto_shared_") || path.starts_with("/home/octo_shared_")
    }

    fn is_shared_linux_user_owner(owner_user_id: Option<&str>) -> bool {
        owner_user_id
            .map(|id| id.starts_with("oqto_shared_") || id.starts_with("octo_shared_"))
            .unwrap_or(false)
    }

    fn validate_record(record: &SessionTargetRecord) -> Result<()> {
        if record.session_id.trim().is_empty() {
            anyhow::bail!("session target session_id must not be empty");
        }

        match record.scope {
            SessionTargetScope::Personal => {
                if record.workspace_id.is_some() {
                    anyhow::bail!(
                        "invalid personal session target: workspace_id must be None (session_id={})",
                        record.session_id
                    );
                }

                if let Some(path) = record.workspace_path.as_deref()
                    && Self::is_shared_workspace_path(path)
                    && !Self::is_shared_linux_user_owner(record.owner_user_id.as_deref())
                {
                    anyhow::bail!(
                        "invalid personal session target: shared workspace path stored as personal (session_id={}, path={})",
                        record.session_id,
                        path
                    );
                }
            }
            SessionTargetScope::SharedWorkspace => {
                if record
                    .workspace_id
                    .as_deref()
                    .map(str::trim)
                    .is_none_or(str::is_empty)
                {
                    anyhow::bail!(
                        "invalid shared session target: workspace_id is required (session_id={})",
                        record.session_id
                    );
                }
            }
        }

        Ok(())
    }

    pub async fn upsert(&self, record: &SessionTargetRecord) -> Result<()> {
        Self::validate_record(record).context("validating chat session target")?;

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
        let row = sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                String,
                Option<String>,
                Option<String>,
            ),
        >(
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

#[cfg(test)]
mod tests {
    use super::{SessionTargetRecord, SessionTargetRepository, SessionTargetScope};

    fn personal_record(path: Option<&str>) -> SessionTargetRecord {
        SessionTargetRecord {
            session_id: "oqto-test-session".to_string(),
            owner_user_id: Some("user_1".to_string()),
            scope: SessionTargetScope::Personal,
            workspace_id: None,
            workspace_path: path.map(str::to_string),
        }
    }

    #[test]
    fn reject_personal_record_with_shared_workspace_path() {
        let record = personal_record(Some("/home/oqto_shared_team/oqto/project"));
        let err = SessionTargetRepository::validate_record(&record).expect_err("must reject");
        assert!(
            err.to_string()
                .contains("shared workspace path stored as personal"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn reject_shared_record_without_workspace_id() {
        let record = SessionTargetRecord {
            session_id: "oqto-test-session".to_string(),
            owner_user_id: None,
            scope: SessionTargetScope::SharedWorkspace,
            workspace_id: None,
            workspace_path: Some("/home/oqto_shared_team/oqto/project".to_string()),
        };
        let err = SessionTargetRepository::validate_record(&record).expect_err("must reject");
        assert!(
            err.to_string().contains("workspace_id is required"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn accept_personal_record_for_shared_linux_owner() {
        let mut record = personal_record(Some("/home/oqto_shared_team/oqto/project"));
        record.owner_user_id = Some("oqto_shared_team".to_string());
        SessionTargetRepository::validate_record(&record)
            .expect("must accept personal record for shared linux owner");
    }

    #[test]
    fn accept_valid_personal_record() {
        let record = personal_record(Some("/home/oqto_usr_wismut/oqto/project"));
        SessionTargetRepository::validate_record(&record).expect("must accept valid record");
    }
}
