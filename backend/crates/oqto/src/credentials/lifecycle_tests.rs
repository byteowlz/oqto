use super::Fixture;
use crate::credentials::*;
use anyhow::Result;
use oqto_protocol::credential_connections::AppConnectionBindRequest;
use std::sync::Arc;

#[tokio::test]
async fn concurrent_cas_and_service_reconstruction_preserve_durable_fences() -> Result<()> {
    let f = Fixture::new().await?;
    assert!(f.bind("a", &f.digest).await?);
    let restarted = ConnectionService::new(
        ConnectionRepository::new(f.db.pool().clone()),
        f.audit.clone(),
    );
    let reference = restarted
        .bound_credential_reference("a", &f.apps, &f.work, &f.instance, "example")
        .await?
        .ok_or_else(|| anyhow::anyhow!("durable binding missing"))?;
    assert!(
        restarted
            .authorize_invocation(
                "a",
                &f.apps,
                &f.work,
                &f.instance,
                "example",
                "/v1/models",
                &reference
            )
            .await?
            .is_some()
    );
    let request = oqto_protocol::credential_connections::ConnectionUpdateRequest {
        expected_revision: 1,
        label: "Concurrent update".into(),
        policy: super::policy(),
    };
    let (first, second) = tokio::join!(
        f.service.update("a", &f.connection.id, &request),
        restarted.update("a", &f.connection.id, &request),
    );
    assert_ne!(first?, second?, "exactly one CAS writer wins");
    assert!(f.invoke("a").await?.is_none());
    Ok(())
}

#[tokio::test]
async fn revision_exhaustion_reserves_terminal_revocation() -> Result<()> {
    let f = Fixture::new().await?;
    sqlx::query("UPDATE credential_connections SET revision=4294967294")
        .execute(f.db.pool())
        .await?;
    let request = oqto_protocol::credential_connections::ConnectionUpdateRequest {
        expected_revision: u32::MAX - 1,
        label: "Exhausted".into(),
        policy: super::policy(),
    };
    assert!(!f.service.update("a", &f.connection.id, &request).await?);
    assert!(
        f.service
            .revoke("a", &f.connection.id, u32::MAX - 1)
            .await?
    );
    assert!(f.service.delete("a", &f.connection.id, u32::MAX).await?);
    Ok(())
}

struct RevokeDuringAudit(sqlx::SqlitePool);
#[async_trait::async_trait]
impl ConnectionAudit for RevokeDuringAudit {
    async fn record(&self, event: &ConnectionAuditEvent) -> Result<()> {
        if matches!(event.action, "bind" | "invoke") {
            sqlx::query("UPDATE app_capability_grants SET revoked_at='revoked-during-audit'")
                .execute(&self.0)
                .await?;
        }
        Ok(())
    }
}

#[tokio::test]
async fn authority_changed_during_audit_cannot_bind_or_invoke() -> Result<()> {
    for action in ["bind", "invoke"] {
        let f = Fixture::new().await?;
        if action == "invoke" {
            assert!(f.bind("a", &f.digest).await?);
        }
        let service = ConnectionService::new(
            ConnectionRepository::new(f.db.pool().clone()),
            Arc::new(RevokeDuringAudit(f.db.pool().clone())),
        );
        if action == "bind" {
            assert!(
                !service
                    .bind(
                        "a",
                        &f.apps,
                        &f.work,
                        &f.instance,
                        "example",
                        &AppConnectionBindRequest {
                            connection_id: f.connection.id,
                            connection_revision: 1,
                            reviewed_content_digest: f.digest,
                        }
                    )
                    .await?
            );
        } else {
            let reference = f
                .service
                .bound_credential_reference("a", &f.apps, &f.work, &f.instance, "example")
                .await?
                .ok_or_else(|| anyhow::anyhow!("missing fixture binding"))?;
            assert!(
                service
                    .authorize_invocation(
                        "a",
                        &f.apps,
                        &f.work,
                        &f.instance,
                        "example",
                        "/v1/models",
                        &reference
                    )
                    .await?
                    .is_none()
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn explicit_rotation_requires_new_connection_revision_and_owner_rebind() -> Result<()> {
    let f = Fixture::new().await?;
    assert!(f.bind("a", &f.digest).await?);
    let mut reference = f
        .service
        .bound_credential_reference("a", &f.apps, &f.work, &f.instance, "example")
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing fixture binding"))?;
    reference.revision = "8".into();
    assert!(
        !f.service
            .update_credential_reference("b", &f.connection.id, 1, &reference)
            .await?
    );
    assert!(
        f.service
            .update_credential_reference("a", &f.connection.id, 1, &reference)
            .await?
    );
    assert!(f.invoke("a").await?.is_none());
    assert!(
        f.service
            .authorize_invocation(
                "a",
                &f.apps,
                &f.work,
                &f.instance,
                "example",
                "/v1/models",
                &reference
            )
            .await?
            .is_none()
    );
    assert!(!f.bind("a", &f.digest).await?);
    assert!(
        f.service
            .bind(
                "a",
                &f.apps,
                &f.work,
                &f.instance,
                "example",
                &AppConnectionBindRequest {
                    connection_id: f.connection.id,
                    connection_revision: 2,
                    reviewed_content_digest: f.digest,
                }
            )
            .await?
    );
    let invocation = f
        .service
        .authorize_invocation(
            "a",
            &f.apps,
            &f.work,
            &f.instance,
            "example",
            "/v1/models",
            &reference,
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("rebound invocation unavailable"))?;
    assert_eq!(invocation.credential().revision, "8");
    assert_eq!(invocation.path(), "/v1/models");
    assert!(!invocation.audit_event_id().is_empty());
    Ok(())
}
