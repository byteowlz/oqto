use super::{CredentialReference, policy::validate_policy};
use anyhow::{Context, Result, ensure};
use oqto_protocol::credential_connections::{
    ConnectionCreateRequest, ConnectionMetadata, ConnectionUpdateRequest,
};
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone)]
pub struct ConnectionRepository {
    pub(super) pool: SqlitePool,
}

#[derive(Debug, FromRow)]
pub(super) struct ConnectionRow {
    pub id: String,
    pub revision: i64,
    pub label: String,
    pub policy_json: String,
    pub provider_id: String,
    pub credential_id: String,
    pub credential_revision: String,
    pub revoked: bool,
}
impl ConnectionRow {
    pub(super) fn metadata(&self) -> Result<ConnectionMetadata> {
        Ok(ConnectionMetadata {
            id: self.id.clone(),
            revision: u32::try_from(self.revision).context("Connection revision overflow")?,
            label: self.label.clone(),
            policy: serde_json::from_str(&self.policy_json).context("decode Connection policy")?,
            revoked: self.revoked,
            rotation_requires_rebind: true,
        })
    }
}
impl ConnectionRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
    pub(super) async fn create(
        &self,
        account: &str,
        id: &str,
        request: &ConnectionCreateRequest,
        credential: &CredentialReference,
    ) -> Result<ConnectionMetadata> {
        validate_policy(&request.policy)?;
        validate_label(&request.label)?;
        validate_credential_reference(credential)?;
        sqlx::query("INSERT INTO credential_connections(id,account_id,revision,label,policy_json,provider_id,credential_id,credential_revision) VALUES(?,?,1,?,?,?,?,?)")
   .bind(id).bind(account).bind(&request.label).bind(serde_json::to_string(&request.policy)?).bind(&credential.provider_id).bind(&credential.credential_id).bind(&credential.revision).execute(&self.pool).await.context("create Account Connection")?;
        self.get(account, id)
            .await?
            .context("created Connection missing")
    }
    pub async fn get(&self, account: &str, id: &str) -> Result<Option<ConnectionMetadata>> {
        self.row(account, id)
            .await?
            .map(|r| r.metadata())
            .transpose()
    }
    pub(super) async fn row(&self, account: &str, id: &str) -> Result<Option<ConnectionRow>> {
        sqlx::query_as("SELECT id,revision,label,policy_json,provider_id,credential_id,credential_revision,revoked FROM credential_connections WHERE account_id=? AND id=?").bind(account).bind(id).fetch_optional(&self.pool).await.context("load owned Connection")
    }
    pub async fn list(&self, account: &str) -> Result<Vec<ConnectionMetadata>> {
        sqlx::query_as::<_,ConnectionRow>("SELECT id,revision,label,policy_json,provider_id,credential_id,credential_revision,revoked FROM credential_connections WHERE account_id=? ORDER BY id").bind(account).fetch_all(&self.pool).await.context("list owned Connections")?.into_iter().map(|r|r.metadata()).collect()
    }
    // Reserve u32::MAX for a final revocation, so revision exhaustion can
    // never make an otherwise live Connection impossible to revoke.
    pub(super) async fn update(
        &self,
        account: &str,
        id: &str,
        request: &ConnectionUpdateRequest,
    ) -> Result<bool> {
        validate_policy(&request.policy)?;
        validate_label(&request.label)?;
        Ok(sqlx::query("UPDATE credential_connections SET revision=revision+1,label=?,policy_json=? WHERE account_id=? AND id=? AND revision=? AND revision<4294967294 AND revoked=0").bind(&request.label).bind(serde_json::to_string(&request.policy)?).bind(account).bind(id).bind(request.expected_revision).execute(&self.pool).await.context("CAS update Connection")?.rows_affected()==1)
    }
    pub(super) async fn update_credential_reference(
        &self,
        account: &str,
        id: &str,
        expected_revision: u32,
        credential: &CredentialReference,
    ) -> Result<bool> {
        validate_credential_reference(credential)?;
        Ok(sqlx::query("UPDATE credential_connections SET revision=revision+1,provider_id=?,credential_id=?,credential_revision=? WHERE account_id=? AND id=? AND revision=? AND revision<4294967294 AND revoked=0")
            .bind(&credential.provider_id).bind(&credential.credential_id).bind(&credential.revision)
            .bind(account).bind(id).bind(expected_revision).execute(&self.pool).await
            .context("CAS replace authenticated Connection credential pin")?.rows_affected()==1)
    }
    pub(super) async fn revoke(&self, account: &str, id: &str, revision: u32) -> Result<bool> {
        Ok(sqlx::query("UPDATE credential_connections SET revision=revision+1,revoked=1 WHERE account_id=? AND id=? AND revision=? AND revision<4294967295 AND revoked=0").bind(account).bind(id).bind(revision).execute(&self.pool).await.context("CAS revoke Connection")?.rows_affected()==1)
    }
    pub(super) async fn delete(&self, account: &str, id: &str, revision: u32) -> Result<bool> {
        Ok(sqlx::query(
            "DELETE FROM credential_connections WHERE account_id=? AND id=? AND revision=?",
        )
        .bind(account)
        .bind(id)
        .bind(revision)
        .execute(&self.pool)
        .await
        .context("CAS delete Connection")?
        .rows_affected()
            == 1)
    }
}
pub(super) fn validate_credential_reference(reference: &CredentialReference) -> Result<()> {
    ensure!(
        [
            &reference.provider_id,
            &reference.credential_id,
            &reference.revision
        ]
        .into_iter()
        .all(|value| !value.is_empty()
            && value.len() <= 256
            && !value.chars().any(char::is_control)),
        "invalid authenticated credential metadata"
    );
    Ok(())
}
fn validate_label(label: &str) -> Result<()> {
    ensure!(
        !label.trim().is_empty() && label.len() <= 128 && !label.chars().any(char::is_control),
        "invalid Connection label"
    );
    Ok(())
}
