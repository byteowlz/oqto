use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

use super::{Grant, GrantRequest};

const GRANT_COLUMNS: &str = "id, workload_id, service_id, workspace_id, resource_id, audience, expires_at, revoked_at, issued_by, created_at";

pub(crate) struct AuditRecord<'a> {
    pub operation: &'a str,
    pub actor: Option<&'a str>,
    pub grant: Option<&'a str>,
    pub decision: &'a str,
    pub reason: &'a str,
    pub occurred_at: i64,
}

#[derive(Clone)]
pub struct PilotRepository {
    pool: SqlitePool,
}

impl PilotRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Current Account activity/role and membership, not token role or admin bypass.
    pub(crate) async fn human_has_access(&self, account: &str, workspace: &str) -> Result<bool> {
        human_has_access(&mut *self.pool.acquire().await?, account, workspace).await
    }

    pub(crate) async fn issuer_has_authority(
        &self,
        account: &str,
        workspace: &str,
    ) -> Result<bool> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM shared_workspaces w JOIN users u ON u.id = w.owner_id JOIN shared_workspace_members m ON m.user_id = u.id AND m.shared_workspace_id = w.id WHERE w.id = ? AND w.owner_id = ? AND u.is_active = 1 AND u.role IN ('user', 'admin'))")
            .bind(workspace).bind(account).fetch_one(&self.pool).await.context("checking pilot issuer authority")
    }

    pub(crate) async fn inspect(&self, workspace: &str) -> Result<Vec<Grant>> {
        sqlx::query_as(&format!("SELECT {GRANT_COLUMNS} FROM service_access_grants WHERE workspace_id = ? ORDER BY created_at, id"))
            .bind(workspace).fetch_all(&self.pool).await.context("inspecting pilot grants")
    }

    pub(crate) async fn workload_grant(&self, credential: &str) -> Result<Option<Grant>> {
        sqlx::query_as(&format!(
            "SELECT {GRANT_COLUMNS} FROM service_access_grants WHERE credential_hash = ?"
        ))
        .bind(hash_credential(credential))
        .fetch_optional(&self.pool)
        .await
        .context("resolving pilot credential")
    }

    /// Transactional owner/membership validation, singleton grant and audit.
    /// SQLite serializes the initial write; concurrent issuance cannot union grants.
    pub(crate) async fn issue(
        &self,
        request: &GrantRequest,
        credential: &str,
        now: i64,
    ) -> Result<Option<Grant>> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("beginning pilot issuance")?;
        // Take the write lock before checking the singleton and owner baseline.
        sqlx::query("UPDATE service_access_grants SET id = id WHERE id = ?")
            .bind(&request.id)
            .execute(&mut *tx)
            .await?;
        let owner: Option<String> =
            sqlx::query_scalar("SELECT owner_id FROM shared_workspaces WHERE id = ?")
                .bind(&request.scope.workspace_id)
                .fetch_optional(&mut *tx)
                .await?;
        if owner.as_deref() != Some(request.owner_account_id.as_str())
            || !human_has_access(
                &mut tx,
                &request.owner_account_id,
                &request.scope.workspace_id,
            )
            .await?
        {
            tx.rollback().await?;
            return Ok(None);
        }
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM service_access_grants WHERE id = ? OR (revoked_at IS NULL AND expires_at > ?))")
            .bind(&request.id).bind(now).fetch_one(&mut *tx).await?;
        if exists {
            tx.rollback().await?;
            return Ok(None);
        }
        sqlx::query("INSERT INTO service_access_grants (id, workload_id, service_id, workspace_id, resource_id, audience, expires_at, credential_hash, issued_by, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&request.id).bind(&request.workload_id).bind(&request.scope.service_id)
            .bind(&request.scope.workspace_id).bind(&request.scope.resource_id).bind(&request.scope.audience)
            .bind(request.expires_at).bind(hash_credential(credential)).bind(&request.owner_account_id).bind(now)
            .execute(&mut *tx).await.context("issuing pilot grant")?;
        audit(
            &mut tx,
            AuditRecord {
                operation: "grant",
                actor: Some("local-admin"),
                grant: Some(&request.id),
                decision: "allow",
                reason: "issued",
                occurred_at: now,
            },
        )
        .await?;
        let grant = sqlx::query_as(&format!(
            "SELECT {GRANT_COLUMNS} FROM service_access_grants WHERE id = ?"
        ))
        .bind(&request.id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await.context("committing pilot grant")?;
        Ok(Some(grant))
    }

    /// Idempotent revocation. No regrant operation can resurrect a revoked ID.
    pub(crate) async fn revoke(&self, id: &str, workspace: &str, now: i64) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query("UPDATE service_access_grants SET revoked_at = COALESCE(revoked_at, ?) WHERE id = ? AND workspace_id = ?")
            .bind(now).bind(id).bind(workspace).execute(&mut *tx).await?;
        let found = result.rows_affected() > 0;
        audit(
            &mut tx,
            AuditRecord {
                operation: "revoke",
                actor: Some("local-admin"),
                grant: Some(id),
                decision: if found { "allow" } else { "deny" },
                reason: if found { "revoked" } else { "not_found" },
                occurred_at: now,
            },
        )
        .await?;
        tx.commit().await?;
        Ok(found)
    }

    pub(crate) async fn audit(&self, record: AuditRecord<'_>) -> Result<String> {
        audit(&mut *self.pool.acquire().await?, record).await
    }

    pub(crate) async fn audit_denial(&self, operation: &str, reason: &str) -> Result<String> {
        self.audit(AuditRecord {
            operation,
            actor: None,
            grant: None,
            decision: "deny",
            reason,
            occurred_at: chrono::Utc::now().timestamp(),
        })
        .await
    }
}

async fn human_has_access(
    connection: &mut SqliteConnection,
    account: &str,
    workspace: &str,
) -> Result<bool> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users u JOIN shared_workspace_members m ON m.user_id = u.id WHERE u.id = ? AND u.is_active = 1 AND u.role IN ('user', 'admin') AND m.shared_workspace_id = ?)")
        .bind(account).bind(workspace).fetch_one(connection).await.context("checking current Account/Workspace authority")
}

async fn audit(connection: &mut SqliteConnection, record: AuditRecord<'_>) -> Result<String> {
    let event_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO service_access_audit (event_id, occurred_at, operation, actor_id, grant_id, decision, reason) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(&event_id).bind(record.occurred_at).bind(record.operation).bind(record.actor).bind(record.grant).bind(record.decision).bind(record.reason)
        .execute(connection).await.context("recording pilot security decision")?;
    Ok(event_id)
}

fn hash_credential(credential: &str) -> String {
    hex::encode(Sha256::digest(credential.as_bytes()))
}
