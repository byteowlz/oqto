use super::{
    ConnectionRepository, CredentialReference, LiveConnectionSlot,
    policy::{permits_path, validate_path, validate_policy},
};
use crate::apps::{AppRuntimeService, AuthorizedWorkDirectory};
use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use oqto_protocol::credential_connections::{
    AppConnectionBindRequest, ConnectionCreateRequest, ConnectionMetadata, ConnectionPolicy,
    ConnectionUpdateRequest,
};
use std::sync::Arc;

/// Parent composition MUST append to canonical oqto-log before returning.
/// There is intentionally no default/no-op audit adapter or SQL audit table.
#[async_trait]
pub trait ConnectionAudit: Send + Sync {
    async fn record(&self, event: &ConnectionAuditEvent) -> Result<()>;
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConnectionAuditEvent {
    /// Canonical oqto-log correlation identity for this attempted action.
    pub event_id: String,
    pub action: &'static str,
    pub account_id: String,
    pub connection_id: String,
    pub revision: Option<u32>,
    pub instance_id: Option<String>,
    pub slot: Option<String>,
    pub content_digest: Option<String>,
}

/// Ephemeral evidence, NOT a bearer grant. The runner must recheck at final
/// attachment and pin the exact provider metadata revision. No secret values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionInvocation {
    account_id: String,
    work_directory_id: String,
    instance_id: String,
    definition_id: String,
    content_digest: String,
    slot: String,
    connection_id: String,
    connection_revision: u32,
    audit_event_id: String,
    path: String,
    policy: ConnectionPolicy,
    credential: CredentialReference,
}
impl ConnectionInvocation {
    pub fn account_id(&self) -> &str {
        &self.account_id
    }
    pub fn work_directory_id(&self) -> &str {
        &self.work_directory_id
    }
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
    pub fn definition_id(&self) -> &str {
        &self.definition_id
    }
    pub fn content_digest(&self) -> &str {
        &self.content_digest
    }
    pub fn slot(&self) -> &str {
        &self.slot
    }
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }
    pub fn connection_revision(&self) -> u32 {
        self.connection_revision
    }
    pub fn audit_event_id(&self) -> &str {
        &self.audit_event_id
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn policy(&self) -> &ConnectionPolicy {
        &self.policy
    }
    pub fn credential(&self) -> &CredentialReference {
        &self.credential
    }
}

#[derive(Clone)]
pub struct ConnectionService {
    repository: ConnectionRepository,
    audit: Arc<dyn ConnectionAudit>,
}
impl ConnectionService {
    #[must_use]
    pub fn new(repository: ConnectionRepository, audit: Arc<dyn ConnectionAudit>) -> Self {
        Self { repository, audit }
    }
    async fn audit(
        &self,
        action: &'static str,
        account: &str,
        id: &str,
        revision: Option<u32>,
        binding: Option<(&str, &str, &str)>,
    ) -> Result<String> {
        let event_id = format!("conn_evt_{}", uuid::Uuid::new_v4().simple());
        self.audit
            .record(&ConnectionAuditEvent {
                event_id: event_id.clone(),
                action,
                account_id: account.to_owned(),
                connection_id: id.to_owned(),
                revision,
                instance_id: binding.map(|b| b.0.to_owned()),
                slot: binding.map(|b| b.1.to_owned()),
                content_digest: binding.map(|b| b.2.to_owned()),
            })
            .await
            .context("persist secret-free Connection audit before action")?;
        Ok(event_id)
    }
    pub async fn create(
        &self,
        account: &str,
        request: &ConnectionCreateRequest,
        credential: &CredentialReference,
    ) -> Result<ConnectionMetadata> {
        validate_policy(&request.policy)?;
        let id = format!("conn_{}", uuid::Uuid::new_v4().simple());
        self.audit("create", account, &id, Some(1), None).await?;
        self.repository
            .create(account, &id, request, credential)
            .await
    }
    pub async fn list(&self, account: &str) -> Result<Vec<ConnectionMetadata>> {
        self.repository.list(account).await
    }
    pub async fn get(&self, account: &str, id: &str) -> Result<Option<ConnectionMetadata>> {
        self.repository.get(account, id).await
    }
    pub async fn update(
        &self,
        account: &str,
        id: &str,
        request: &ConnectionUpdateRequest,
    ) -> Result<bool> {
        validate_policy(&request.policy)?;
        if self.get(account, id).await?.is_none() {
            return Ok(false);
        }
        self.audit("update", account, id, Some(request.expected_revision), None)
            .await?;
        self.repository.update(account, id, request).await
    }
    /// Explicit host-authenticated rotation. This bumps the Connection revision,
    /// invalidating every old slot binding until its owner explicitly rebinds.
    pub async fn update_credential_reference(
        &self,
        account: &str,
        id: &str,
        expected_revision: u32,
        credential: &CredentialReference,
    ) -> Result<bool> {
        super::repository::validate_credential_reference(credential)?;
        if self.get(account, id).await?.is_none() {
            return Ok(false);
        }
        self.audit("rotate", account, id, Some(expected_revision), None)
            .await?;
        self.repository
            .update_credential_reference(account, id, expected_revision, credential)
            .await
    }
    pub async fn revoke(&self, account: &str, id: &str, revision: u32) -> Result<bool> {
        if self.get(account, id).await?.is_none() {
            return Ok(false);
        }
        self.audit("revoke", account, id, Some(revision), None)
            .await?;
        self.repository.revoke(account, id, revision).await
    }
    pub async fn delete(&self, account: &str, id: &str, revision: u32) -> Result<bool> {
        if self.get(account, id).await?.is_none() {
            return Ok(false);
        }
        self.audit("delete", account, id, Some(revision), None)
            .await?;
        self.repository.delete(account, id, revision).await
    }
    pub async fn bind(
        &self,
        account: &str,
        apps: &AppRuntimeService,
        work_directory: &AuthorizedWorkDirectory,
        instance: &str,
        slot: &str,
        request: &AppConnectionBindRequest,
    ) -> Result<bool> {
        let Some(live) = apps
            .live_connection_slot(account, work_directory, instance, slot)
            .await?
        else {
            return Ok(false);
        };
        let Some(connection) = self.repository.row(account, &request.connection_id).await? else {
            return Ok(false);
        };
        let metadata = connection.metadata()?;
        if metadata.revoked
            || metadata.revision != request.connection_revision
            || live.content_digest != request.reviewed_content_digest
            || !matches_slot(&live, &metadata.policy)
        {
            return Ok(false);
        }
        self.audit(
            "bind",
            account,
            &metadata.id,
            Some(metadata.revision),
            Some((instance, slot, &live.content_digest)),
        )
        .await?;
        // Recheck after audit and use a conditional atomic INSERT. A racing
        // revocation/Definition/owner/grant change cannot install new authority.
        let Some(refreshed) = apps
            .live_connection_slot(account, work_directory, instance, slot)
            .await?
        else {
            return Ok(false);
        };
        if refreshed != live {
            return Ok(false);
        }
        let affected=sqlx::query(r#"
INSERT INTO app_connection_bindings(account_id,instance_id,slot,connection_id,connection_revision,content_digest)
SELECT ?,i.id,?,c.id,c.revision,d.content_digest
FROM app_instances i JOIN app_definitions d ON d.id=i.definition_id
JOIN app_installations installation ON installation.id=i.installation_id
JOIN work_directories w ON w.id=i.binding_id
JOIN app_capability_grants g ON g.instance_id=i.id
JOIN credential_connections c ON c.id=? AND c.account_id=?
WHERE i.id=? AND i.created_by_account_id=? AND i.status='active'
AND i.binding_kind='work_directory' AND i.binding_id=?
AND installation.owner_kind='work_directory' AND installation.owner_id=w.id
AND installation.current_definition_id=d.id
AND w.owner_kind=? AND w.owner_id=? AND w.binding_path=?
AND d.id=? AND d.content_digest=? AND d.requested_capabilities_json=?
AND g.definition_id=d.id AND g.content_digest=d.content_digest
AND g.decided_by_account_id=? AND g.decision='allowed' AND g.revoked_at IS NULL AND g.request_json=?
AND c.revoked=0 AND c.revision=?
ON CONFLICT(account_id,instance_id,slot) DO UPDATE SET
connection_id=excluded.connection_id,connection_revision=excluded.connection_revision,content_digest=excluded.content_digest
"#).bind(account).bind(slot).bind(&metadata.id).bind(account).bind(instance).bind(account).bind(&work_directory.id)
.bind(match work_directory.owner_kind { oqto_protocol::apps::AppOwnerKind::Account=>"account",oqto_protocol::apps::AppOwnerKind::Workspace=>"workspace",_=>"unsupported" })
.bind(&work_directory.owner_id).bind(work_directory.root.to_str()).bind(&live.definition_id).bind(&live.content_digest).bind(&live.canonical_json)
.bind(account).bind(&live.grant_json).bind(metadata.revision).execute(&self.repository.pool).await.context("bind Connection to live granted App slot")?.rows_affected();
        Ok(affected == 1)
    }
    pub async fn unbind(&self, account: &str, instance: &str, slot: &str) -> Result<bool> {
        let binding = sqlx::query_as::<_, (String, i64, String)>(
            "SELECT connection_id,connection_revision,content_digest FROM app_connection_bindings WHERE account_id=? AND instance_id=? AND slot=?",
        ).bind(account).bind(instance).bind(slot).fetch_optional(&self.repository.pool)
            .await.context("read owned slot before unbinding")?;
        let Some((connection_id, revision, digest)) = binding else {
            return Ok(false);
        };
        self.audit(
            "unbind",
            account,
            &connection_id,
            Some(u32::try_from(revision)?),
            Some((instance, slot, &digest)),
        )
        .await?;
        Ok(sqlx::query(
            "DELETE FROM app_connection_bindings WHERE account_id=? AND instance_id=? AND slot=? AND connection_id=? AND connection_revision=? AND content_digest=?",
        )
        .bind(account)
        .bind(instance)
        .bind(slot)
        .bind(connection_id)
        .bind(revision)
        .bind(digest)
        .execute(&self.repository.pool)
        .await
        .context("unbind owned Connection slot")?
        .rows_affected()
            == 1)
    }
    pub async fn authorize_invocation(
        &self,
        account: &str,
        apps: &AppRuntimeService,
        work_directory: &AuthorizedWorkDirectory,
        instance: &str,
        slot: &str,
        path: &str,
        current_credential: &CredentialReference,
    ) -> Result<Option<ConnectionInvocation>> {
        validate_path(path)?;
        let Some(evidence) = self
            .invocation(
                account,
                apps,
                work_directory,
                instance,
                slot,
                path,
                current_credential,
            )
            .await?
        else {
            return Ok(None);
        };
        let audit_event_id = self
            .audit(
                "invoke",
                account,
                &evidence.connection_id,
                Some(evidence.connection_revision),
                Some((instance, slot, &evidence.content_digest)),
            )
            .await?;
        let refreshed = self
            .invocation(
                account,
                apps,
                work_directory,
                instance,
                slot,
                path,
                current_credential,
            )
            .await?;
        Ok(refreshed.filter(|new| new == &evidence).map(|mut checked| {
            checked.audit_event_id = audit_event_id;
            checked
        }))
    }
    /// Obtain metadata to ask the authenticated provider for its current
    /// revision. This does not grant use and never resolves secret bytes.
    pub async fn bound_credential_reference(
        &self,
        account: &str,
        apps: &AppRuntimeService,
        work_directory: &AuthorizedWorkDirectory,
        instance: &str,
        slot: &str,
    ) -> Result<Option<CredentialReference>> {
        Ok(self
            .bound_connection(account, apps, work_directory, instance, slot)
            .await?
            .map(|(_, row)| CredentialReference {
                provider_id: row.provider_id,
                credential_id: row.credential_id,
                revision: row.credential_revision,
            }))
    }

    async fn bound_connection(
        &self,
        account: &str,
        apps: &AppRuntimeService,
        work_directory: &AuthorizedWorkDirectory,
        instance: &str,
        slot: &str,
    ) -> Result<Option<(LiveConnectionSlot, super::repository::ConnectionRow)>> {
        let Some(live) = apps
            .live_connection_slot(account, work_directory, instance, slot)
            .await?
        else {
            return Ok(None);
        };
        // One SQLite snapshot joins binding, Connection and all live App facts.
        // The initial helper supplies the validated semantic request, never
        // an independently issued Connection grant.
        let row = sqlx::query_as::<_, super::repository::ConnectionRow>(r#"
SELECT c.id,c.revision,c.label,c.policy_json,c.provider_id,c.credential_id,c.credential_revision,c.revoked
FROM app_connection_bindings b
JOIN credential_connections c ON c.id=b.connection_id AND c.account_id=b.account_id
JOIN app_instances i ON i.id=b.instance_id
JOIN app_definitions d ON d.id=i.definition_id
JOIN app_installations installation ON installation.id=i.installation_id
JOIN work_directories w ON w.id=i.binding_id
JOIN app_capability_grants g ON g.instance_id=i.id
WHERE b.account_id=? AND b.instance_id=? AND b.slot=?
AND b.connection_revision=c.revision AND b.content_digest=d.content_digest AND c.revoked=0
AND i.created_by_account_id=b.account_id AND i.status='active'
AND i.binding_kind='work_directory' AND i.binding_id=?
AND installation.owner_kind='work_directory' AND installation.owner_id=w.id
AND installation.current_definition_id=d.id
AND w.owner_kind=? AND w.owner_id=? AND w.binding_path=?
AND d.id=? AND d.content_digest=? AND d.requested_capabilities_json=?
AND g.definition_id=d.id AND g.content_digest=d.content_digest
AND g.decided_by_account_id=b.account_id AND g.decision='allowed'
AND g.revoked_at IS NULL AND g.request_json=?
"#)
        .bind(account).bind(instance).bind(slot).bind(&work_directory.id)
        .bind(match work_directory.owner_kind { oqto_protocol::apps::AppOwnerKind::Account => "account", oqto_protocol::apps::AppOwnerKind::Workspace => "workspace", _ => "unsupported" })
        .bind(&work_directory.owner_id).bind(work_directory.root.to_str())
        .bind(&live.definition_id).bind(&live.content_digest).bind(&live.canonical_json).bind(&live.grant_json)
        .fetch_optional(&self.repository.pool).await.context("read live bound Connection authority")?;
        let Some(row) = row else {
            return Ok(None);
        };
        if !matches_slot(&live, &row.metadata()?.policy) {
            return Ok(None);
        }
        Ok(Some((live, row)))
    }

    async fn invocation(
        &self,
        account: &str,
        apps: &AppRuntimeService,
        work_directory: &AuthorizedWorkDirectory,
        instance: &str,
        slot: &str,
        path: &str,
        current_credential: &CredentialReference,
    ) -> Result<Option<ConnectionInvocation>> {
        let Some((live, row)) = self
            .bound_connection(account, apps, work_directory, instance, slot)
            .await?
        else {
            return Ok(None);
        };
        let metadata = row.metadata()?;
        let credential = CredentialReference {
            provider_id: row.provider_id,
            credential_id: row.credential_id,
            revision: row.credential_revision,
        };
        if metadata.revoked
            || &credential != current_credential
            || !matches_slot(&live, &metadata.policy)
            || !permits_path(&metadata.policy.path_prefix, path)
        {
            return Ok(None);
        }
        ensure!(metadata.revision > 0, "invalid stored revision");
        Ok(Some(ConnectionInvocation {
            account_id: account.to_owned(),
            work_directory_id: work_directory.id.clone(),
            instance_id: instance.to_owned(),
            definition_id: live.definition_id,
            content_digest: live.content_digest,
            slot: slot.to_owned(),
            connection_id: metadata.id,
            connection_revision: metadata.revision,
            audit_event_id: String::new(),
            path: path.to_owned(),
            policy: metadata.policy,
            credential,
        }))
    }
}
fn matches_slot(live: &LiveConnectionSlot, policy: &ConnectionPolicy) -> bool {
    validate_policy(policy).is_ok()
        && live.request.profile == "https_get_v1"
        && (live.request.expected_origin == format!("https://{}", policy.host)
            || live.request.expected_origin == format!("https://{}:443", policy.host))
}
