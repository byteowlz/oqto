use anyhow::{Context, Result};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use super::models::{
    AppInstanceRow, CapabilityGrantRow, DefinitionPermissionRow, PublicationInsert,
};

/// One permission decision plus the Instance and workflow states it implies.
#[derive(Debug, Clone)]
pub struct PermissionDecisionInsert<'a> {
    pub instance_id: &'a str,
    pub definition_id: &'a str,
    pub content_digest: &'a str,
    pub request_json: &'a str,
    pub decision: &'a str,
    pub decided_by_account_id: &'a str,
    pub instance_status: &'a str,
    pub workflow_status: &'a str,
}

#[derive(Debug, Clone)]
pub struct AppRepository {
    pool: SqlitePool,
}

impl AppRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert_work_directory(
        &self,
        owner_kind: &str,
        owner_id: &str,
        binding_path: &str,
    ) -> Result<String> {
        let proposed_id = format!("wdir_{}", Uuid::new_v4().simple());
        let row = sqlx::query(
            r#"
            INSERT INTO work_directories (id, owner_kind, owner_id, binding_path)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(owner_kind, owner_id, binding_path)
            DO UPDATE SET updated_at = datetime('now')
            RETURNING id
            "#,
        )
        .bind(&proposed_id)
        .bind(owner_kind)
        .bind(owner_id)
        .bind(binding_path)
        .fetch_one(&self.pool)
        .await
        .context("upserting work-directory public identity")?;
        row.try_get("id")
            .context("reading work-directory public identity")
    }

    pub async fn create_workflow(
        &self,
        acting_account_id: &str,
        work_directory_id: &str,
        app_id: &str,
    ) -> Result<String> {
        let workflow_id = format!("appwf_{}", Uuid::new_v4().simple());
        sqlx::query(
            r#"
            INSERT INTO app_publish_workflows (
                id, acting_account_id, work_directory_id, app_id, status
            ) VALUES (?, ?, ?, ?, 'validating')
            "#,
        )
        .bind(&workflow_id)
        .bind(acting_account_id)
        .bind(work_directory_id)
        .bind(app_id)
        .execute(&self.pool)
        .await
        .context("creating App publish workflow")?;
        Ok(workflow_id)
    }

    pub async fn get_instance_kv(
        &self,
        instance_id: &str,
        account_id: &str,
        key: &str,
    ) -> Result<Option<serde_json::Value>> {
        let row = sqlx::query(
            "SELECT value_json FROM app_instance_kv WHERE instance_id = ? AND account_id = ? AND key = ?",
        )
        .bind(instance_id)
        .bind(account_id)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .context("reading App Instance KV")?;
        row.map(|row| {
            let json: String = row
                .try_get("value_json")
                .context("reading App Instance KV JSON")?;
            serde_json::from_str(&json).context("decoding App Instance KV JSON")
        })
        .transpose()
    }

    pub async fn set_instance_kv(
        &self,
        instance_id: &str,
        account_id: &str,
        key: &str,
        value: &serde_json::Value,
    ) -> Result<()> {
        let json = serde_json::to_string(value).context("encoding App Instance KV JSON")?;
        sqlx::query(
            r#"
            INSERT INTO app_instance_kv (instance_id, account_id, key, value_json)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(instance_id, account_id, key) DO UPDATE SET
                value_json = excluded.value_json,
                version = app_instance_kv.version + 1,
                updated_at = datetime('now')
            "#,
        )
        .bind(instance_id)
        .bind(account_id)
        .bind(key)
        .bind(json)
        .execute(&self.pool)
        .await
        .context("writing App Instance KV")?;
        Ok(())
    }

    pub async fn delete_instance_kv(
        &self,
        instance_id: &str,
        account_id: &str,
        key: &str,
    ) -> Result<()> {
        sqlx::query(
            "DELETE FROM app_instance_kv WHERE instance_id = ? AND account_id = ? AND key = ?",
        )
        .bind(instance_id)
        .bind(account_id)
        .bind(key)
        .execute(&self.pool)
        .await
        .context("deleting App Instance KV")?;
        Ok(())
    }

    pub async fn update_workflow_status(&self, workflow_id: &str, status: &str) -> Result<()> {
        sqlx::query(
            "UPDATE app_publish_workflows SET status = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(status)
        .bind(workflow_id)
        .execute(&self.pool)
        .await
        .context("updating App publish workflow status")?;
        Ok(())
    }

    pub async fn fail_workflow(
        &self,
        workflow_id: &str,
        rejection_code: &str,
        rejection_message: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE app_publish_workflows
            SET status = 'failed', rejection_code = ?, rejection_message = ?,
                updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(rejection_code)
        .bind(rejection_message)
        .bind(workflow_id)
        .execute(&self.pool)
        .await
        .context("failing App publish workflow")?;
        Ok(())
    }

    pub async fn commit_publication(
        &self,
        workflow_id: &str,
        publication: &PublicationInsert<'_>,
    ) -> Result<AppInstanceRow> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .context("beginning App publication transaction")?;
        let definition = &publication.definition;

        sqlx::query(
            r#"
            INSERT INTO app_definitions (
                id, content_digest, app_id, version, title_json, description,
                schema_tag, manifest_toml, web_entry_path, file_index_json,
                requested_capabilities_json, total_bytes
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(content_digest) DO NOTHING
            "#,
        )
        .bind(definition.id)
        .bind(definition.content_digest)
        .bind(definition.app_id)
        .bind(definition.version)
        .bind(definition.title_json)
        .bind(definition.description)
        .bind(definition.schema_tag)
        .bind(definition.manifest_toml)
        .bind(definition.web_entry_path)
        .bind(definition.file_index_json)
        .bind(definition.requested_capabilities_json)
        .bind(definition.total_bytes)
        .execute(&mut *transaction)
        .await
        .context("recording immutable App Definition")?;

        let existing_installation = sqlx::query(
            r#"
            SELECT id FROM app_installations
            WHERE owner_kind = 'work_directory' AND owner_id = ? AND app_id = ?
            "#,
        )
        .bind(publication.work_directory_id)
        .bind(publication.app_id)
        .fetch_optional(&mut *transaction)
        .await
        .context("looking up App Installation")?;

        let installation_id = if let Some(row) = existing_installation {
            let id: String = row.try_get("id").context("reading App Installation id")?;
            sqlx::query(
                r#"
                UPDATE app_installations
                SET current_definition_id = ?, source_work_directory_id = ?,
                    source_package_name = ?, updated_at = datetime('now')
                WHERE id = ?
                "#,
            )
            .bind(definition.id)
            .bind(publication.work_directory_id)
            .bind(publication.package_name)
            .bind(&id)
            .execute(&mut *transaction)
            .await
            .context("updating App Installation Definition")?;
            id
        } else {
            let id = format!("appinstl_{}", Uuid::new_v4().simple());
            let provenance = serde_json::json!({
                "kind": "work_directory_source",
                "work_directory_id": publication.work_directory_id,
                "package_name": publication.package_name,
            });
            sqlx::query(
                r#"
                INSERT INTO app_installations (
                    id, owner_kind, owner_id, app_id, current_definition_id,
                    source_work_directory_id, source_package_name, provenance_json
                ) VALUES (?, 'work_directory', ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(publication.work_directory_id)
            .bind(publication.app_id)
            .bind(definition.id)
            .bind(publication.work_directory_id)
            .bind(publication.package_name)
            .bind(provenance.to_string())
            .execute(&mut *transaction)
            .await
            .context("creating App Installation")?;
            id
        };

        let existing_instance = sqlx::query(
            r#"
            SELECT id FROM app_instances
            WHERE installation_id = ? AND definition_id = ?
              AND binding_kind = 'work_directory' AND binding_id = ?
            "#,
        )
        .bind(&installation_id)
        .bind(definition.id)
        .bind(publication.work_directory_id)
        .fetch_optional(&mut *transaction)
        .await
        .context("looking up pinned App Instance")?;

        // Reusing an existing Instance deliberately leaves its status alone:
        // republishing identical source must never re-activate an Instance a
        // person denied or revoked.
        let instance_id = if let Some(row) = existing_instance {
            row.try_get("id").context("reading App Instance id")?
        } else {
            let id = format!("appinstance_{}", Uuid::new_v4().simple());
            sqlx::query(
                r#"
                INSERT INTO app_instances (
                    id, installation_id, definition_id, binding_kind, binding_id,
                    status, created_by_account_id
                ) VALUES (?, ?, ?, 'work_directory', ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(&installation_id)
            .bind(definition.id)
            .bind(publication.work_directory_id)
            .bind(publication.initial_instance_status)
            .bind(publication.acting_account_id)
            .execute(&mut *transaction)
            .await
            .context("creating pinned App Instance")?;
            id
        };

        sqlx::query(
            r#"
            UPDATE app_publish_workflows
            SET status = ?, definition_id = ?, installation_id = ?,
                instance_id = ?, updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(publication.workflow_status)
        .bind(definition.id)
        .bind(&installation_id)
        .bind(&instance_id)
        .bind(workflow_id)
        .execute(&mut *transaction)
        .await
        .context("completing App publish workflow")?;

        transaction
            .commit()
            .await
            .context("committing App publication transaction")?;

        self.get_instance(&instance_id)
            .await?
            .context("published App Instance missing after commit")
    }

    pub async fn list_instances_for_work_directory(
        &self,
        work_directory_id: &str,
    ) -> Result<Vec<AppInstanceRow>> {
        sqlx::query_as::<_, AppInstanceRow>(INSTANCE_SELECT)
            .bind(work_directory_id)
            .fetch_all(&self.pool)
            .await
            .context("listing App Instances for work directory")
    }

    pub async fn get_instance(&self, instance_id: &str) -> Result<Option<AppInstanceRow>> {
        sqlx::query_as::<_, AppInstanceRow>(GET_INSTANCE)
            .bind(instance_id)
            .fetch_optional(&self.pool)
            .await
            .context("loading App Instance")
    }

    pub async fn get_instance_for_work_directory(
        &self,
        instance_id: &str,
        work_directory_id: &str,
    ) -> Result<Option<AppInstanceRow>> {
        sqlx::query_as::<_, AppInstanceRow>(GET_INSTANCE_FOR_WORK_DIRECTORY)
            .bind(instance_id)
            .bind(work_directory_id)
            .fetch_optional(&self.pool)
            .await
            .context("loading authorized work-directory App Instance")
    }
}

impl AppRepository {
    /// Definition facts the permission flow needs, including the canonical
    /// capability request pinned at publication.
    pub async fn get_definition_permissions(
        &self,
        definition_id: &str,
    ) -> Result<Option<DefinitionPermissionRow>> {
        sqlx::query_as::<_, DefinitionPermissionRow>(
            r#"
            SELECT id AS definition_id, content_digest, app_id, version, title_json,
                   requested_capabilities_json
            FROM app_definitions
            WHERE id = ?
            "#,
        )
        .bind(definition_id)
        .fetch_optional(&self.pool)
        .await
        .context("loading App Definition capability request")
    }

    pub async fn get_capability_grant(
        &self,
        instance_id: &str,
    ) -> Result<Option<CapabilityGrantRow>> {
        sqlx::query_as::<_, CapabilityGrantRow>(
            r#"
            SELECT instance_id, definition_id, content_digest, request_json, decision,
                   decided_by_account_id, decided_at, revoked_at
            FROM app_capability_grants
            WHERE instance_id = ?
            "#,
        )
        .bind(instance_id)
        .fetch_optional(&self.pool)
        .await
        .context("loading App capability grant")
    }

    /// Record one permission decision and move the Instance in the same
    /// transaction, so an Instance can never be active without a live grant.
    pub async fn record_permission_decision(
        &self,
        decision: &PermissionDecisionInsert<'_>,
    ) -> Result<()> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .context("beginning App permission decision transaction")?;

        let grant_id = format!("appgrant_{}", Uuid::new_v4().simple());
        sqlx::query(
            r#"
            INSERT INTO app_capability_grants (
                id, instance_id, definition_id, content_digest, request_json,
                decision, decided_by_account_id, decided_at, revoked_at, revoked_by_account_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'), NULL, NULL)
            ON CONFLICT(instance_id) DO UPDATE SET
                definition_id = excluded.definition_id,
                content_digest = excluded.content_digest,
                request_json = excluded.request_json,
                decision = excluded.decision,
                decided_by_account_id = excluded.decided_by_account_id,
                decided_at = datetime('now'),
                revoked_at = NULL,
                revoked_by_account_id = NULL
            "#,
        )
        .bind(&grant_id)
        .bind(decision.instance_id)
        .bind(decision.definition_id)
        .bind(decision.content_digest)
        .bind(decision.request_json)
        .bind(decision.decision)
        .bind(decision.decided_by_account_id)
        .execute(&mut *transaction)
        .await
        .context("recording App permission decision")?;

        sqlx::query(
            "UPDATE app_instances SET status = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(decision.instance_status)
        .bind(decision.instance_id)
        .execute(&mut *transaction)
        .await
        .context("applying App permission decision to the Instance")?;

        sqlx::query(
            r#"
            UPDATE app_publish_workflows
            SET status = ?, updated_at = datetime('now')
            WHERE instance_id = ?
            "#,
        )
        .bind(decision.workflow_status)
        .bind(decision.instance_id)
        .execute(&mut *transaction)
        .await
        .context("recording App permission decision on the publish workflow")?;

        transaction
            .commit()
            .await
            .context("committing App permission decision")
    }

    /// Revoke a live grant and suspend the Instance in one transaction.
    pub async fn revoke_capability_grant(
        &self,
        instance_id: &str,
        revoked_by_account_id: &str,
    ) -> Result<bool> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .context("beginning App grant revocation transaction")?;

        let revoked = sqlx::query(
            r#"
            UPDATE app_capability_grants
            SET revoked_at = datetime('now'), revoked_by_account_id = ?
            WHERE instance_id = ? AND decision = 'allowed' AND revoked_at IS NULL
            "#,
        )
        .bind(revoked_by_account_id)
        .bind(instance_id)
        .execute(&mut *transaction)
        .await
        .context("revoking App capability grant")?
        .rows_affected()
            > 0;

        if !revoked {
            transaction
                .rollback()
                .await
                .context("rolling back a revocation with no live grant")?;
            return Ok(false);
        }

        sqlx::query(
            r#"
            UPDATE app_instances
            SET status = 'suspended', updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(instance_id)
        .execute(&mut *transaction)
        .await
        .context("suspending the Instance whose grant was revoked")?;

        transaction
            .commit()
            .await
            .context("committing App grant revocation")?;
        Ok(true)
    }

    pub async fn get_definition_assets(
        &self,
        definition_id: &str,
    ) -> Result<Option<super::models::DefinitionAssetRow>> {
        sqlx::query_as::<_, super::models::DefinitionAssetRow>(
            r#"
            SELECT id AS definition_id, content_digest, web_entry_path, file_index_json
            FROM app_definitions
            WHERE id = ?
            "#,
        )
        .bind(definition_id)
        .fetch_optional(&self.pool)
        .await
        .context("loading App Definition assets")
    }
}

const GET_INSTANCE: &str = r#"
    SELECT
        ai.id AS instance_id,
        ai.definition_id,
        ai.installation_id,
        ad.app_id,
        ad.version,
        ad.title_json,
        ad.content_digest,
        ains.owner_kind AS installation_owner_kind,
        ai.binding_kind,
        ai.status
    FROM app_instances ai
    JOIN app_definitions ad ON ad.id = ai.definition_id
    JOIN app_installations ains ON ains.id = ai.installation_id
    WHERE ai.id = ?
"#;

const GET_INSTANCE_FOR_WORK_DIRECTORY: &str = r#"
    SELECT
        ai.id AS instance_id,
        ai.definition_id,
        ai.installation_id,
        ad.app_id,
        ad.version,
        ad.title_json,
        ad.content_digest,
        ains.owner_kind AS installation_owner_kind,
        ai.binding_kind,
        ai.status
    FROM app_instances ai
    JOIN app_definitions ad ON ad.id = ai.definition_id
    JOIN app_installations ains ON ains.id = ai.installation_id
    WHERE ai.id = ? AND ai.binding_kind = 'work_directory' AND ai.binding_id = ?
"#;

const INSTANCE_SELECT: &str = r#"
    SELECT
        ai.id AS instance_id,
        ai.definition_id,
        ai.installation_id,
        ad.app_id,
        ad.version,
        ad.title_json,
        ad.content_digest,
        ains.owner_kind AS installation_owner_kind,
        ai.binding_kind,
        ai.status
    FROM app_instances ai
    JOIN app_definitions ad ON ad.id = ai.definition_id
    JOIN app_installations ains ON ains.id = ai.installation_id
    WHERE ai.binding_kind = 'work_directory' AND ai.binding_id = ?
    ORDER BY ad.app_id ASC, ai.created_at DESC
    "#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::models::DefinitionInsert;
    use crate::db::Database;

    /// Canonical request used by the capability-requesting fixtures.
    const CAPABILITY_REQUEST: &str = r#"[{"capability":"theme"}]"#;

    fn definition<'a>(id: &'a str, digest: &'a str, version: &'a str) -> DefinitionInsert<'a> {
        definition_with(id, digest, version, "[]")
    }

    fn definition_with<'a>(
        id: &'a str,
        digest: &'a str,
        version: &'a str,
        requested_capabilities_json: &'a str,
    ) -> DefinitionInsert<'a> {
        DefinitionInsert {
            id,
            content_digest: digest,
            app_id: "hello",
            version,
            title_json: r#"{"en":"Hello"}"#,
            description: None,
            schema_tag: "oqto-app/v0",
            manifest_toml: "schema = \"oqto-app/v0\"",
            web_entry_path: "bundle/index.html",
            file_index_json: "[]",
            requested_capabilities_json,
            total_bytes: 0,
        }
    }

    fn publication<'a>(
        work_directory_id: &'a str,
        definition: DefinitionInsert<'a>,
        initial_instance_status: &'a str,
        workflow_status: &'a str,
    ) -> PublicationInsert<'a> {
        PublicationInsert {
            acting_account_id: "acct-1",
            work_directory_id,
            package_name: "hello.oqtoapp",
            app_id: "hello",
            definition,
            initial_instance_status,
            workflow_status,
        }
    }

    /// Publish one capability-requesting App and return its Instance.
    async fn publish_awaiting(
        repository: &AppRepository,
        work_directory_id: &str,
        definition_id: &str,
        digest: &str,
    ) -> Result<AppInstanceRow> {
        let workflow = repository
            .create_workflow("acct-1", work_directory_id, "hello")
            .await?;
        repository
            .commit_publication(
                &workflow,
                &publication(
                    work_directory_id,
                    definition_with(definition_id, digest, "0.1.0", CAPABILITY_REQUEST),
                    "awaiting_permission",
                    "awaiting_permission",
                ),
            )
            .await
    }

    async fn decide(
        repository: &AppRepository,
        instance: &AppInstanceRow,
        decision: &str,
        instance_status: &str,
        workflow_status: &str,
    ) -> Result<()> {
        repository
            .record_permission_decision(&PermissionDecisionInsert {
                instance_id: &instance.instance_id,
                definition_id: &instance.definition_id,
                content_digest: &instance.content_digest,
                request_json: CAPABILITY_REQUEST,
                decision,
                decided_by_account_id: "acct-1",
                instance_status,
                workflow_status,
            })
            .await
    }

    #[tokio::test]
    async fn unchanged_publish_is_idempotent_and_changed_definition_stays_pinned() -> Result<()> {
        let database = Database::in_memory().await?;
        let repository = AppRepository::new(database.pool().clone());
        let work_directory_id = repository
            .upsert_work_directory("account", "acct-1", "/workspace")
            .await?;

        let first_workflow = repository
            .create_workflow("acct-1", &work_directory_id, "hello")
            .await?;
        let first = repository
            .commit_publication(
                &first_workflow,
                &publication(
                    &work_directory_id,
                    definition("appdef_a", &"a".repeat(64), "0.1.0"),
                    "active",
                    "instance_ready",
                ),
            )
            .await?;

        let repeat_workflow = repository
            .create_workflow("acct-1", &work_directory_id, "hello")
            .await?;
        let repeat = repository
            .commit_publication(
                &repeat_workflow,
                &publication(
                    &work_directory_id,
                    definition("appdef_a", &"a".repeat(64), "0.1.0"),
                    "active",
                    "instance_ready",
                ),
            )
            .await?;
        assert_eq!(repeat.instance_id, first.instance_id);
        assert_eq!(repeat.definition_id, first.definition_id);

        let changed_workflow = repository
            .create_workflow("acct-1", &work_directory_id, "hello")
            .await?;
        let changed = repository
            .commit_publication(
                &changed_workflow,
                &publication(
                    &work_directory_id,
                    definition("appdef_b", &"b".repeat(64), "0.2.0"),
                    "active",
                    "instance_ready",
                ),
            )
            .await?;
        assert_ne!(changed.instance_id, first.instance_id);
        assert_ne!(changed.definition_id, first.definition_id);

        let instances = repository
            .list_instances_for_work_directory(&work_directory_id)
            .await?;
        assert_eq!(instances.len(), 2);
        assert!(
            instances
                .iter()
                .any(|instance| instance.definition_id == "appdef_a")
        );
        assert!(
            instances
                .iter()
                .any(|instance| instance.definition_id == "appdef_b")
        );

        Ok(())
    }

    #[tokio::test]
    async fn zero_capability_publish_is_active_and_ungranted() -> Result<()> {
        let database = Database::in_memory().await?;
        let repository = AppRepository::new(database.pool().clone());
        let work_directory_id = repository
            .upsert_work_directory("account", "acct-1", "/workspace")
            .await?;
        let workflow = repository
            .create_workflow("acct-1", &work_directory_id, "hello")
            .await?;

        let instance = repository
            .commit_publication(
                &workflow,
                &publication(
                    &work_directory_id,
                    definition("appdef_a", &"a".repeat(64), "0.1.0"),
                    "active",
                    "instance_ready",
                ),
            )
            .await?;

        assert_eq!(instance.status, "active");
        assert!(
            repository
                .get_capability_grant(&instance.instance_id)
                .await?
                .is_none(),
            "an App that asks for nothing must hold no grant row"
        );
        Ok(())
    }

    #[tokio::test]
    async fn capability_publish_awaits_permission_until_allowed() -> Result<()> {
        let database = Database::in_memory().await?;
        let repository = AppRepository::new(database.pool().clone());
        let work_directory_id = repository
            .upsert_work_directory("account", "acct-1", "/workspace")
            .await?;

        let instance =
            publish_awaiting(&repository, &work_directory_id, "appdef_a", &"a".repeat(64)).await?;
        assert_eq!(instance.status, "awaiting_permission");
        assert!(
            repository
                .get_capability_grant(&instance.instance_id)
                .await?
                .is_none()
        );

        decide(&repository, &instance, "allowed", "active", "approved").await?;

        let allowed = repository
            .get_instance(&instance.instance_id)
            .await?
            .context("instance after allow")?;
        assert_eq!(allowed.status, "active");

        let grant = repository
            .get_capability_grant(&instance.instance_id)
            .await?
            .context("grant after allow")?;
        assert_eq!(grant.decision, "allowed");
        assert_eq!(grant.decided_by_account_id, "acct-1");
        assert_eq!(grant.content_digest, instance.content_digest);
        assert_eq!(grant.request_json, CAPABILITY_REQUEST);
        assert!(grant.revoked_at.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn deny_suspends_and_republish_never_reactivates() -> Result<()> {
        let database = Database::in_memory().await?;
        let repository = AppRepository::new(database.pool().clone());
        let work_directory_id = repository
            .upsert_work_directory("account", "acct-1", "/workspace")
            .await?;

        let instance =
            publish_awaiting(&repository, &work_directory_id, "appdef_a", &"a".repeat(64)).await?;
        decide(&repository, &instance, "denied", "suspended", "denied").await?;

        let denied = repository
            .get_instance(&instance.instance_id)
            .await?
            .context("instance after deny")?;
        assert_eq!(denied.status, "suspended");

        // Republishing byte-identical source reuses the same pinned Instance
        // and must not resurrect it.
        let republished =
            publish_awaiting(&repository, &work_directory_id, "appdef_a", &"a".repeat(64)).await?;
        assert_eq!(republished.instance_id, instance.instance_id);
        assert_eq!(
            republished.status, "suspended",
            "republishing must never re-activate a denied Instance"
        );

        let grant = repository
            .get_capability_grant(&instance.instance_id)
            .await?
            .context("grant after republish")?;
        assert_eq!(grant.decision, "denied");
        Ok(())
    }

    #[tokio::test]
    async fn revoke_suspends_immediately_and_is_not_repeatable() -> Result<()> {
        let database = Database::in_memory().await?;
        let repository = AppRepository::new(database.pool().clone());
        let work_directory_id = repository
            .upsert_work_directory("account", "acct-1", "/workspace")
            .await?;

        let instance =
            publish_awaiting(&repository, &work_directory_id, "appdef_a", &"a".repeat(64)).await?;
        decide(&repository, &instance, "allowed", "active", "approved").await?;

        assert!(
            repository
                .revoke_capability_grant(&instance.instance_id, "acct-2")
                .await?
        );
        let revoked = repository
            .get_instance(&instance.instance_id)
            .await?
            .context("instance after revoke")?;
        assert_eq!(revoked.status, "suspended");

        let grant = repository
            .get_capability_grant(&instance.instance_id)
            .await?
            .context("grant after revoke")?;
        assert!(grant.revoked_at.is_some());

        assert!(
            !repository
                .revoke_capability_grant(&instance.instance_id, "acct-2")
                .await?,
            "revoking twice must report that no live grant remained"
        );
        Ok(())
    }

    #[tokio::test]
    async fn instance_kv_is_private_to_account_and_instance() -> Result<()> {
        let database = Database::in_memory().await?;
        let repository = AppRepository::new(database.pool().clone());
        let work_directory_id = repository
            .upsert_work_directory("account", "acct-1", "/workspace")
            .await?;
        let first =
            publish_awaiting(&repository, &work_directory_id, "appdef_a", &"a".repeat(64)).await?;
        let second =
            publish_awaiting(&repository, &work_directory_id, "appdef_b", &"b".repeat(64)).await?;

        repository
            .set_instance_kv(
                &first.instance_id,
                "acct-1",
                "counter",
                &serde_json::json!(7),
            )
            .await?;
        assert_eq!(
            repository
                .get_instance_kv(&first.instance_id, "acct-1", "counter")
                .await?,
            Some(serde_json::json!(7))
        );
        assert_eq!(
            repository
                .get_instance_kv(&first.instance_id, "acct-2", "counter")
                .await?,
            None
        );
        assert_eq!(
            repository
                .get_instance_kv(&second.instance_id, "acct-1", "counter")
                .await?,
            None
        );

        repository
            .delete_instance_kv(&first.instance_id, "acct-1", "counter")
            .await?;
        assert_eq!(
            repository
                .get_instance_kv(&first.instance_id, "acct-1", "counter")
                .await?,
            None
        );
        Ok(())
    }

    #[tokio::test]
    async fn instances_are_scoped_to_their_work_directory() -> Result<()> {
        let database = Database::in_memory().await?;
        let repository = AppRepository::new(database.pool().clone());
        let mine = repository
            .upsert_work_directory("account", "acct-1", "/workspace")
            .await?;
        let other = repository
            .upsert_work_directory("account", "acct-2", "/other")
            .await?;

        let instance = publish_awaiting(&repository, &mine, "appdef_a", &"a".repeat(64)).await?;

        assert!(
            repository
                .get_instance_for_work_directory(&instance.instance_id, &mine)
                .await?
                .is_some()
        );
        assert!(
            repository
                .get_instance_for_work_directory(&instance.instance_id, &other)
                .await?
                .is_none(),
            "an Instance must not be reachable from another work directory"
        );
        Ok(())
    }
}
