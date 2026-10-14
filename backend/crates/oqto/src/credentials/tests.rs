use super::*;
#[path = "lifecycle_tests.rs"]
mod lifecycle_tests;
use crate::{
    apps::{AppArtifactStore, AppRepository, AppRuntimeService, AuthorizedWorkDirectory},
    db::Database,
};
use anyhow::{Context, Result, bail};
use oqto_protocol::{
    apps::{AppCapabilityRequest, AppOwnerKind},
    credential_connections::*,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
struct Audit {
    fail: AtomicBool,
}
#[async_trait::async_trait]
impl ConnectionAudit for Audit {
    async fn record(&self, event: &ConnectionAuditEvent) -> Result<()> {
        let json = serde_json::to_string(event)?;
        assert!(!json.contains("credential_id") && !json.contains("path_prefix"));
        if self.fail.load(Ordering::SeqCst) {
            bail!("audit unavailable");
        }
        Ok(())
    }
}
fn policy() -> ConnectionPolicy {
    ConnectionPolicy {
        host: "api.example.com".into(),
        port: 443,
        path_prefix: "/v1/".into(),
        method: "GET".into(),
        header: "authorization".into(),
        attachment_prefix: "Bearer ".into(),
    }
}
fn credential() -> CredentialReference {
    CredentialReference {
        provider_id: "local".into(),
        credential_id: "synthetic-entry".into(),
        revision: "7".into(),
    }
}
fn create() -> ConnectionCreateRequest {
    ConnectionCreateRequest {
        label: "Example".into(),
        policy: policy(),
    }
}

#[tokio::test]
async fn crud_cas_owner_revision_and_audit_fail_closed() -> Result<()> {
    let db = Database::in_memory().await?;
    let repo = ConnectionRepository::new(db.pool().clone());
    let audit = Arc::new(Audit::default());
    let service = ConnectionService::new(repo, audit.clone());
    audit.fail.store(true, Ordering::SeqCst);
    assert!(service.create("a", &create(), &credential()).await.is_err());
    assert!(service.list("a").await?.is_empty());
    audit.fail.store(false, Ordering::SeqCst);
    let first = service.create("a", &create(), &credential()).await?;
    let public = serde_json::to_string(&first)?;
    assert!(
        !public.contains("provider_id")
            && !public.contains("credential_id")
            && !public.contains("synthetic-entry")
    );
    assert!(service.list("b").await?.is_empty());
    assert!(service.get("b", &first.id).await?.is_none());
    let update = ConnectionUpdateRequest {
        expected_revision: 1,
        label: "Updated".into(),
        policy: policy(),
    };
    assert!(!service.update("b", &first.id, &update).await?);
    assert!(service.update("a", &first.id, &update).await?);
    assert!(!service.update("a", &first.id, &update).await?);
    assert!(!service.revoke("a", &first.id, 1).await?);
    audit.fail.store(true, Ordering::SeqCst);
    assert!(service.revoke("a", &first.id, 2).await.is_err());
    assert!(
        !service
            .get("a", &first.id)
            .await?
            .context("owned Connection missing")?
            .revoked
    );
    audit.fail.store(false, Ordering::SeqCst);
    assert!(service.revoke("a", &first.id, 2).await?);
    assert!(!service.revoke("a", &first.id, 3).await?);
    assert!(service.delete("a", &first.id, 3).await?);
    let replacement = service.create("a", &create(), &credential()).await?;
    assert_ne!(first.id, replacement.id);
    Ok(())
}

struct Fixture {
    db: Database,
    apps: AppRuntimeService,
    work: AuthorizedWorkDirectory,
    instance: String,
    service: ConnectionService,
    audit: Arc<Audit>,
    connection: ConnectionMetadata,
    digest: String,
}
impl Fixture {
    async fn new() -> Result<Self> {
        let db = Database::in_memory().await?;
        let repository = AppRepository::new(db.pool().clone());
        let w = repository
            .upsert_work_directory("account", "a", "/synthetic")
            .await?;
        let request = serde_json::to_string(&vec![AppCapabilityRequest::Connections {
            slots: vec![AppConnectionSlot {
                slot: "example".into(),
                expected_origin: "https://api.example.com".into(),
                profile: "https_get_v1".into(),
            }],
        }])?;
        let digest = "a".repeat(64);
        sqlx::query("INSERT INTO app_definitions(id,content_digest,app_id,version,title_json,schema_tag,manifest_toml,web_entry_path,file_index_json,total_bytes,requested_capabilities_json) VALUES('d',?,'fixture','0.1.0','{}','oqto-app/v0','','bundle/index.html','[]',0,?)").bind(&digest).bind(&request).execute(db.pool()).await?;
        sqlx::query("INSERT INTO app_installations(id,owner_kind,owner_id,app_id,current_definition_id,provenance_json) VALUES('installation','work_directory',?,'fixture','d','{}')").bind(&w).execute(db.pool()).await?;
        sqlx::query("INSERT INTO app_instances(id,installation_id,definition_id,binding_kind,binding_id,status,created_by_account_id) VALUES('instance','installation','d','work_directory',?,'active','a')").bind(&w).execute(db.pool()).await?;
        sqlx::query("INSERT INTO app_capability_grants(id,instance_id,definition_id,content_digest,request_json,decision,decided_by_account_id) VALUES('grant','instance','d',?,?,'allowed','a')").bind(&digest).bind(&request).execute(db.pool()).await?;
        let apps = AppRuntimeService::new(
            repository,
            AppArtifactStore::new("/synthetic-artifacts".into()),
        );
        let work = AuthorizedWorkDirectory {
            id: w,
            owner_kind: AppOwnerKind::Account,
            owner_id: "a".into(),
            root: "/synthetic".into(),
            plane: Arc::new(crate::user_plane::RunnerUserPlane::new(
                oqto_runner::client::RunnerClient::new("/synthetic-never-connected.sock"),
            )),
        };
        let audit = Arc::new(Audit::default());
        let service =
            ConnectionService::new(ConnectionRepository::new(db.pool().clone()), audit.clone());
        let connection = service.create("a", &create(), &credential()).await?;
        Ok(Self {
            db,
            apps,
            work,
            instance: "instance".into(),
            service,
            audit,
            connection,
            digest,
        })
    }
    async fn bind(&self, account: &str, digest: &str) -> Result<bool> {
        self.service
            .bind(
                account,
                &self.apps,
                &self.work,
                &self.instance,
                "example",
                &AppConnectionBindRequest {
                    connection_id: self.connection.id.clone(),
                    connection_revision: self.connection.revision,
                    reviewed_content_digest: digest.into(),
                },
            )
            .await
    }
    async fn invoke(&self, account: &str) -> Result<Option<ConnectionInvocation>> {
        self.service
            .authorize_invocation(
                account,
                &self.apps,
                &self.work,
                &self.instance,
                "example",
                "/v1/models",
                &credential(),
            )
            .await
    }
}

#[tokio::test]
async fn binding_and_invocation_require_exact_owner_digest_slot_policy_and_rotation() -> Result<()>
{
    let f = Fixture::new().await?;
    assert!(f.invoke("a").await?.is_none());
    assert!(!f.bind("b", &f.digest).await?);
    assert!(!f.bind("a", "wrong-digest").await?);
    assert!(f.bind("a", &f.digest).await?);
    assert!(f.invoke("a").await?.is_some());
    assert!(f.invoke("b").await?.is_none());
    let mut changed = credential();
    changed.revision = "8".into();
    assert!(
        f.service
            .authorize_invocation(
                "a",
                &f.apps,
                &f.work,
                &f.instance,
                "example",
                "/v1/models",
                &changed
            )
            .await?
            .is_none()
    );
    assert!(
        f.service
            .authorize_invocation(
                "a",
                &f.apps,
                &f.work,
                &f.instance,
                "example",
                "/v10/models",
                &credential()
            )
            .await?
            .is_none()
    );
    assert!(
        f.service
            .authorize_invocation(
                "a",
                &f.apps,
                &f.work,
                &f.instance,
                "missing",
                "/v1/models",
                &credential()
            )
            .await?
            .is_none()
    );
    f.audit.fail.store(true, Ordering::SeqCst);
    assert!(f.invoke("a").await.is_err());
    f.audit.fail.store(false, Ordering::SeqCst);
    let update = ConnectionUpdateRequest {
        expected_revision: 1,
        label: "Changed".into(),
        policy: policy(),
    };
    assert!(f.service.update("a", &f.connection.id, &update).await?);
    assert!(f.invoke("a").await?.is_none());
    assert!(!f.bind("a", &f.digest).await?);
    assert!(f.service.delete("a", &f.connection.id, 2).await?);
    assert!(f.invoke("a").await?.is_none());
    assert!(f.service.create("a", &create(), &credential()).await?.id != f.connection.id);
    assert!(f.invoke("a").await?.is_none());
    Ok(())
}

#[tokio::test]
async fn live_authority_lifecycle_and_grant_mismatches() -> Result<()> {
    for mutation in [
        "UPDATE app_instances SET status='suspended'",
        "UPDATE app_instances SET created_by_account_id='b'",
        "UPDATE work_directories SET owner_id='b'",
        "UPDATE work_directories SET binding_path='/other'",
        "UPDATE app_capability_grants SET revoked_at='now'",
        "UPDATE app_capability_grants SET decision='denied'",
        "UPDATE app_capability_grants SET decided_by_account_id='b'",
        "UPDATE app_capability_grants SET content_digest='wrong'",
        "UPDATE app_capability_grants SET request_json='[]'",
        "UPDATE app_capability_grants SET definition_id='d2'",
        "UPDATE app_instances SET definition_id='d2'",
        "UPDATE app_installations SET current_definition_id='d2'",
        "UPDATE app_installations SET owner_id='other-work-directory'",
        "UPDATE app_instances SET binding_id='other-work-directory'",
        "UPDATE app_definitions SET content_digest='wrong' WHERE id='d'",
        "DELETE FROM app_instances",
    ] {
        let f = Fixture::new().await?;
        assert!(f.bind("a", &f.digest).await?);
        sqlx::query("INSERT INTO app_definitions(id,content_digest,app_id,version,title_json,schema_tag,manifest_toml,web_entry_path,file_index_json,total_bytes,requested_capabilities_json) SELECT 'd2','other-digest',app_id,version,title_json,schema_tag,manifest_toml,web_entry_path,file_index_json,total_bytes,requested_capabilities_json FROM app_definitions WHERE id='d'")
            .execute(f.db.pool()).await?;
        if mutation == "DELETE FROM app_instances" {
            sqlx::query("DELETE FROM app_capability_grants")
                .execute(f.db.pool())
                .await?;
        }
        sqlx::query(mutation).execute(f.db.pool()).await?;
        assert!(f.invoke("a").await?.is_none(), "mutation {mutation}");
        assert!(!f.bind("a", &f.digest).await?, "mutation {mutation}");
    }
    Ok(())
}

#[tokio::test]
async fn revoke_unbind_and_destination_change_do_not_reissue_app_grants() -> Result<()> {
    let f = Fixture::new().await?;
    assert!(f.bind("a", &f.digest).await?);
    assert!(!f.service.unbind("b", &f.instance, "example").await?);
    assert!(f.service.unbind("a", &f.instance, "example").await?);
    assert!(f.invoke("a").await?.is_none());
    assert!(f.bind("a", &f.digest).await?);
    assert!(f.service.revoke("a", &f.connection.id, 1).await?);
    assert!(f.invoke("a").await?.is_none());
    assert!(!f.bind("a", &f.digest).await?);
    let mut other = create();
    other.policy.host = "evil.example.com".into();
    let destination = f.service.create("a", &other, &credential()).await?;
    assert!(
        !f.service
            .bind(
                "a",
                &f.apps,
                &f.work,
                &f.instance,
                "example",
                &AppConnectionBindRequest {
                    connection_id: destination.id,
                    connection_revision: 1,
                    reviewed_content_digest: f.digest
                }
            )
            .await?
    );
    let grant = f
        .apps
        .repository()
        .get_capability_grant(&f.instance)
        .await?
        .context("original App grant missing")?;
    assert_eq!(grant.decision, "allowed");
    assert!(grant.revoked_at.is_none());
    Ok(())
}

#[test]
fn unsupported_policy_and_encoded_paths_fail_closed() {
    for host in [
        "127.0.0.1",
        "127.1",
        "0x7f.0x1",
        "localhost",
        "API.example.com",
        "api.example.com.",
        "api..example.com",
    ] {
        let mut p = policy();
        p.host = host.into();
        assert!(policy::validate_policy(&p).is_err());
    }
    for path in [
        "//evil",
        "/v1/../admin",
        "/v1/%2e%2e",
        "/v1?url=evil",
        "/v1\\admin",
    ] {
        assert!(policy::validate_path(path).is_err());
    }
    let mut p = policy();
    p.method = "POST".into();
    assert!(policy::validate_policy(&p).is_err());
    p = policy();
    p.header = "cookie".into();
    assert!(policy::validate_policy(&p).is_err());
    p = policy();
    p.port = 8443;
    assert!(policy::validate_policy(&p).is_err());
    p = policy();
    p.attachment_prefix = "Basic ".into();
    assert!(policy::validate_policy(&p).is_err());
}
