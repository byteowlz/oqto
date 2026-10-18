use std::{path::Path, time::Duration};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use crate::{Binding, Capability, Operation, Policy, RuleMode, Target, policy::digest};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestState {
    Pending,
    Approved,
    Denied,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub operation: Operation,
    pub binding: Binding,
    pub policy_revision: String,
    pub digest: String,
    pub revision: u64,
    pub state: RequestState,
    pub expires_at: u64,
    pub grant_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantLifetime {
    Once,
    Session,
}

/// A runtime-only delivery fence; callers cannot deserialize a forged Permit.
#[derive(Clone, Debug, Serialize)]
pub struct Permit {
    request_id: String,
    operation_digest: String,
    authority_fingerprint: String,
    generation: i64,
    grant_id: Option<String>,
    expires_at: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum Admission {
    Allowed(Permit),
    Pending(Box<Request>),
    Denied,
}

/// An admitted invocation whose owner died before confirming completion.
/// Its effects are unknown; it is never replayed and must be inspected.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interrupted {
    pub operation: Operation,
    pub started_at: u64,
}

impl Permit {
    /// Admission deadline for a trusted supervisor; not a transferable grant.
    pub fn expires_at(&self) -> u64 {
        self.expires_at
    }
}

/// One protected authority bound to one verified Session/audience/policy.
/// SQLite owns permission state only; it never stores provider data or Chat.
#[derive(Clone)]
pub struct Authority {
    pool: SqlitePool,
    binding: Binding,
    policy: Policy,
    fingerprint: String,
}

impl Authority {
    pub async fn open(path: &Path, binding: Binding, policy: Policy) -> Result<Self> {
        Self::open_inner(path, binding, policy, false).await
    }

    /// Owner-only: adopt a changed ceiling for the same verified binding. All
    /// overlays are revoked and pending requests discarded in one transaction,
    /// so nothing approved under the previous ceiling survives. A different
    /// binding/audience still fails closed.
    pub async fn open_replacing_policy(
        path: &Path,
        binding: Binding,
        policy: Policy,
    ) -> Result<Self> {
        Self::open_inner(path, binding, policy, true).await
    }

    async fn open_inner(
        path: &Path,
        binding: Binding,
        policy: Policy,
        replace: bool,
    ) -> Result<Self> {
        binding.validate()?;
        policy.validate()?;
        prepare_private_file(path)?;
        let options = SqliteConnectOptions::new()
            .filename(path)
            .busy_timeout(Duration::from_secs(3))
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        sqlx::raw_sql("CREATE TABLE IF NOT EXISTS authority (singleton INTEGER PRIMARY KEY CHECK(singleton=1), fingerprint TEXT NOT NULL, ledger_id TEXT NOT NULL, generation INTEGER NOT NULL, clock INTEGER NOT NULL, binding TEXT NOT NULL DEFAULT '');
            CREATE TABLE IF NOT EXISTS requests (id TEXT PRIMARY KEY, digest TEXT NOT NULL, state TEXT NOT NULL, revision INTEGER NOT NULL, document TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS grants (id TEXT PRIMARY KEY, target TEXT NOT NULL, operation TEXT, expires INTEGER NOT NULL, revoked INTEGER NOT NULL DEFAULT 0, resource TEXT);
            CREATE TABLE IF NOT EXISTS invocations (id TEXT PRIMARY KEY, operation TEXT NOT NULL, started INTEGER NOT NULL, state TEXT NOT NULL CHECK(state IN ('live','interrupted')));
            CREATE TABLE IF NOT EXISTS operator_receipts (id TEXT PRIMARY KEY, digest TEXT NOT NULL);")
            .execute(&pool).await?;
        // Ledgers created before these columns existed: migrate once, under an
        // exclusive transaction so concurrent openers cannot both alter.
        let mut migration = pool.begin_with("BEGIN IMMEDIATE").await?;
        for (table, column) in [("authority", "binding"), ("grants", "resource")] {
            let present: i64 = sqlx::query_scalar(&format!(
                "SELECT count(*) FROM pragma_table_info('{table}') WHERE name='{column}'"
            ))
            .fetch_one(&mut *migration)
            .await?;
            if present == 0 {
                sqlx::raw_sql(&format!(
                    "ALTER TABLE {table} ADD COLUMN {column} TEXT{}",
                    if column == "binding" {
                        " NOT NULL DEFAULT ''"
                    } else {
                        ""
                    }
                ))
                .execute(&mut *migration)
                .await?;
            }
        }
        migration.commit().await?;
        let fingerprint = digest(&(&binding, &policy))?;
        let binding_digest = digest(&binding)?;
        sqlx::query(
            "INSERT OR IGNORE INTO authority (singleton, fingerprint, ledger_id, generation, clock, binding) VALUES (1, ?, lower(hex(randomblob(16))), 0, 0, ?)",
        )
        .bind(&fingerprint)
        .bind(&binding_digest)
        .execute(&pool)
        .await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT fingerprint, binding FROM authority WHERE singleton=1")
            .fetch_one(&mut *tx)
            .await?;
        let saved: String = row.get("fingerprint");
        if saved != fingerprint {
            ensure!(
                replace && row.get::<String, _>("binding") == binding_digest,
                "authority binding/audience/policy mismatch"
            );
            sqlx::query("UPDATE grants SET revoked=1 WHERE revoked=0")
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM requests WHERE state='pending'")
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE authority SET fingerprint=? WHERE singleton=1")
                .bind(&fingerprint)
                .execute(&mut *tx)
                .await?;
            advance(&mut tx).await?;
        }
        tx.commit().await?;
        let ledger_id: String =
            sqlx::query_scalar("SELECT ledger_id FROM authority WHERE singleton=1")
                .fetch_one(&pool)
                .await?;
        // Same binding in a different store is not the same authority. This
        // stable namespace also prevents replaying its reviewed request hash.
        let fingerprint = digest(&(&ledger_id, &binding, &policy))?;
        Ok(Self {
            pool,
            binding,
            policy,
            fingerprint,
        })
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }

    /// `now` and operation correlation must come from the trusted runner, not
    /// model inputs. An approval is not permission to change arguments.
    pub async fn admit(&self, operation: Operation, now: u64) -> Result<Admission> {
        operation.validate()?;
        let hash = digest(&(&self.fingerprint, &operation))?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let (generation, now) = clock(&mut tx, now).await?;
        if let Some(row) = sqlx::query("SELECT digest, document FROM requests WHERE id=?")
            .bind(&operation.request_id)
            .fetch_optional(&mut *tx)
            .await?
        {
            ensure!(
                row.get::<String, _>("digest") == hash,
                "request identity reused with changed arguments"
            );
            let mut request: Request = serde_json::from_str(&row.get::<String, _>("document"))?;
            let result = match request.state {
                RequestState::Denied => Admission::Denied,
                RequestState::Pending if request.expires_at > now => {
                    if let Some((id, expires)) =
                        matching_grant(&mut tx, &operation, now, None).await?
                    {
                        let previous = request.revision;
                        request.revision = previous
                            .checked_add(1)
                            .context("request revision exhausted")?;
                        request.state = RequestState::Approved;
                        request.grant_id = Some(id.clone());
                        request.expires_at = expires;
                        let changed = sqlx::query("UPDATE requests SET state='approved', revision=?, document=? WHERE id=? AND state='pending' AND revision=?")
                            .bind(i64::try_from(request.revision)?).bind(serde_json::to_string(&request)?)
                            .bind(&operation.request_id).bind(i64::try_from(previous)?)
                            .execute(&mut *tx).await?.rows_affected();
                        ensure!(changed == 1, "Session grant admission lost CAS");
                        Admission::Allowed(Permit {
                            request_id: operation.request_id,
                            operation_digest: hash,
                            authority_fingerprint: self.fingerprint.clone(),
                            generation,
                            grant_id: Some(id),
                            expires_at: expires,
                        })
                    } else {
                        Admission::Pending(Box::new(request))
                    }
                }
                RequestState::Approved
                    if self.policy.mode(&operation.target) == RuleMode::Allow
                        && request.grant_id.is_none() =>
                {
                    Admission::Allowed(Permit {
                        request_id: operation.request_id,
                        operation_digest: hash,
                        authority_fingerprint: self.fingerprint.clone(),
                        generation,
                        grant_id: None,
                        expires_at: u64::MAX,
                    })
                }
                RequestState::Approved if self.policy.mode(&operation.target) == RuleMode::Ask => {
                    match matching_grant(&mut tx, &operation, now, request.grant_id.as_deref())
                        .await?
                    {
                        Some((id, expires)) if request.grant_id.as_ref() == Some(&id) => {
                            Admission::Allowed(Permit {
                                request_id: operation.request_id,
                                operation_digest: hash,
                                authority_fingerprint: self.fingerprint.clone(),
                                generation,
                                grant_id: Some(id),
                                expires_at: expires,
                            })
                        }
                        _ => Admission::Denied,
                    }
                }
                _ => Admission::Denied,
            };
            tx.commit().await?;
            return Ok(result);
        }
        let result = match self.policy.mode(&operation.target) {
            RuleMode::Deny => Admission::Denied,
            RuleMode::Allow => {
                let request = Request {
                    operation: operation.clone(),
                    binding: self.binding.clone(),
                    policy_revision: self.policy.revision.clone(),
                    digest: hash.clone(),
                    revision: 1,
                    state: RequestState::Approved,
                    expires_at: u64::MAX,
                    grant_id: None,
                };
                insert_request(&mut tx, &request).await?;
                Admission::Allowed(Permit {
                    request_id: operation.request_id,
                    operation_digest: hash,
                    authority_fingerprint: self.fingerprint.clone(),
                    generation,
                    grant_id: None,
                    expires_at: u64::MAX,
                })
            }
            RuleMode::Ask => {
                let grant = matching_grant(&mut tx, &operation, now, None).await?;
                let (state, grant_id, expires_at) = match &grant {
                    Some((id, expires)) => (RequestState::Approved, Some(id.clone()), *expires),
                    None => (
                        RequestState::Pending,
                        None,
                        now.checked_add(300_000)
                            .context("pending deadline overflow")?,
                    ),
                };
                let request = Request {
                    operation: operation.clone(),
                    binding: self.binding.clone(),
                    policy_revision: self.policy.revision.clone(),
                    digest: hash,
                    revision: 1,
                    state,
                    expires_at,
                    grant_id,
                };
                insert_request(&mut tx, &request).await?;
                match grant {
                    Some((id, expires)) => Admission::Allowed(Permit {
                        request_id: operation.request_id,
                        operation_digest: request.digest,
                        authority_fingerprint: self.fingerprint.clone(),
                        generation,
                        grant_id: Some(id),
                        expires_at: expires,
                    }),
                    None => Admission::Pending(Box::new(request)),
                }
            }
        };
        tx.commit().await?;
        Ok(result)
    }

    /// Operator-only control API. The carrier must authenticate `operator`;
    /// the crate checks its authority against the pinned Session owner.
    pub async fn decide(
        &self,
        operator: &str,
        id: &str,
        reviewed_digest: &str,
        expected_revision: u64,
        lifetime: GrantLifetime,
        allow: bool,
        expires_at: u64,
        now: u64,
    ) -> Result<Request> {
        ensure!(
            operator == self.binding.account_id,
            "operator cannot approve this Session"
        );
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let (_, now) = clock(&mut tx, now).await?;
        let row = sqlx::query("SELECT document FROM requests WHERE id=?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        let mut request: Request = serde_json::from_str(&row.get::<String, _>("document"))?;
        ensure!(
            request.digest == reviewed_digest
                && request.revision == expected_revision
                && request.state == RequestState::Pending,
            "stale or unreviewed permission request"
        );
        ensure!(request.expires_at > now, "permission request expired");
        ensure!(
            self.policy.mode(&request.operation.target) == RuleMode::Ask,
            "permission is outside requestable ceiling"
        );
        if allow {
            ensure!(
                expires_at > now && expires_at <= now.saturating_add(86_400_000),
                "grant expiry must be within 24 hours"
            );
            let target_hash = digest(&request.operation.target)?;
            let operation = match lifetime {
                GrantLifetime::Once => Some(request.operation.request_id.clone()),
                GrantLifetime::Session => None,
            };
            let grant_id = digest(&(&request.digest, request.revision, lifetime, expires_at))?;
            sqlx::query("INSERT INTO grants (id, target, operation, expires, resource) VALUES (?, ?, ?, ?, ?)")
                .bind(&grant_id)
                .bind(target_hash)
                .bind(operation)
                .bind(i64::try_from(expires_at)?)
                .bind(serde_json::to_string(&request.operation.target)?)
                .execute(&mut *tx)
                .await?;
            request.state = RequestState::Approved;
            request.grant_id = Some(grant_id);
            request.expires_at = expires_at;
        } else {
            request.state = RequestState::Denied;
        }
        request.revision = request
            .revision
            .checked_add(1)
            .context("request revision exhausted")?;
        let changed = sqlx::query("UPDATE requests SET state=?, revision=?, document=? WHERE id=? AND state='pending' AND revision=? AND digest=?")
            .bind(state_name(request.state)).bind(i64::try_from(request.revision)?)
            .bind(serde_json::to_string(&request)?).bind(id)
            .bind(i64::try_from(expected_revision)?).bind(reviewed_digest)
            .execute(&mut *tx).await?.rows_affected();
        ensure!(changed == 1, "permission decision lost CAS");
        advance(&mut tx).await?;
        tx.commit().await?;
        Ok(request)
    }

    pub async fn inspect(&self, id: &str) -> Result<Request> {
        let document: String = sqlx::query_scalar("SELECT document FROM requests WHERE id=?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        Ok(serde_json::from_str(&document)?)
    }

    pub async fn revoke(&self, operator: &str, grant_id: &str) -> Result<()> {
        ensure!(
            operator == self.binding.account_id,
            "operator cannot revoke this Session"
        );
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let count = sqlx::query("UPDATE grants SET revoked=1 WHERE id=? AND revoked=0")
            .bind(grant_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        ensure!(count == 1, "unknown or already revoked grant");
        advance(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Operator-only revocation of this bound Session's approved overlays.
    /// Baseline policy is not an overlay and remains pinned.
    pub async fn revoke_all(&self, operator: &str) -> Result<u64> {
        ensure!(
            operator == self.binding.account_id,
            "operator cannot revoke this Session"
        );
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let count = sqlx::query("UPDATE grants SET revoked=1 WHERE revoked=0")
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if count > 0 {
            advance(&mut tx).await?;
        }
        tx.commit().await?;
        Ok(count)
    }

    /// Check a permit and durably journal the invocation as live in one
    /// transaction. A journal entry that outlives its owner becomes
    /// `Interrupted` on the next `recover`, never a silent replay.
    pub async fn begin(&self, permit: &Permit, operation: &Operation, now: u64) -> Result<()> {
        let mut tx = self.permit_check(permit, operation, now).await?;
        let changed = sqlx::query(
            "INSERT INTO invocations (id, operation, started, state) VALUES (?, ?, ?, 'live') ON CONFLICT(id) DO NOTHING",
        )
        .bind(&operation.request_id)
        .bind(serde_json::to_string(operation)?)
        .bind(i64::try_from(now)?)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        ensure!(
            changed == 1,
            "invocation already journaled; refusing re-entry"
        );
        tx.commit().await?;
        Ok(())
    }

    /// Trusted owner effect journal, not permission admission. Expose only on
    /// the protected operator channel; never on a model/tool transport.
    pub async fn begin_operator(
        &self,
        operator: &str,
        operation: &Operation,
        now: u64,
    ) -> Result<()> {
        ensure!(operator == self.binding.account_id, "operator mismatch");
        ensure!(
            operation
                .request_id
                .starts_with(&format!("{}:operator:", self.binding.session_id)),
            "operator identity mismatch"
        );
        ensure!(
            operation.target.capability == Capability::Execute
                && operation.target.resource_id == "operator-shell",
            "invalid operator target"
        );
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let prefix = format!("{}:operator:", self.binding.session_id);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM (SELECT id FROM invocations WHERE instr(id, ?)=1 UNION SELECT id FROM operator_receipts)")
            .bind(&prefix).fetch_one(&mut *tx).await?;
        ensure!(
            count < 128,
            "operator receipt capacity exhausted; open a new Session"
        );
        let completed: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM operator_receipts WHERE id=?")
                .bind(&operation.request_id)
                .fetch_one(&mut *tx)
                .await?;
        ensure!(
            completed == 0,
            "operator invocation already has a result receipt"
        );
        let changed = sqlx::query("INSERT INTO invocations (id, operation, started, state) VALUES (?, ?, ?, 'live') ON CONFLICT(id) DO NOTHING")
            .bind(&operation.request_id)
            .bind(serde_json::to_string(operation)?)
            .bind(i64::try_from(now)?)
            .execute(&mut *tx).await?.rows_affected();
        ensure!(changed == 1, "operator invocation already journaled");
        tx.commit().await?;
        Ok(())
    }

    /// Seal only a result fingerprint after confirmed executor drain. The native
    /// worker owns result bytes/Chat; this table is protected effect evidence.
    pub async fn seal_operator_result(
        &self,
        operator: &str,
        id: &str,
        fingerprint: &str,
    ) -> Result<()> {
        self.operator_receipt_binding(operator, id, fingerprint)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let live: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM invocations WHERE id=? AND state='live'")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        ensure!(live == 1, "operator result has no live invocation");
        sqlx::query(
            "INSERT INTO operator_receipts(id,digest) VALUES (?,?) ON CONFLICT(id) DO NOTHING",
        )
        .bind(id)
        .bind(fingerprint)
        .execute(&mut *tx)
        .await?;
        let sealed: String = sqlx::query_scalar("SELECT digest FROM operator_receipts WHERE id=?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        ensure!(
            sealed == fingerprint,
            "operator result changed after sealing"
        );
        tx.commit().await?;
        Ok(())
    }

    /// A worker acknowledgment attests native durable admission, not UI display.
    /// Exact sealed receipt reconciliation may clear an interrupted row, but
    /// never restores grants. The tombstone forbids re-entry after completion.
    pub async fn complete_operator_result(
        &self,
        operator: &str,
        id: &str,
        fingerprint: &str,
    ) -> Result<()> {
        self.operator_receipt_binding(operator, id, fingerprint)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let sealed: Option<String> =
            sqlx::query_scalar("SELECT digest FROM operator_receipts WHERE id=?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        ensure!(
            sealed.as_deref() == Some(fingerprint),
            "unknown or changed operator receipt"
        );
        sqlx::query("DELETE FROM invocations WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    fn operator_receipt_binding(&self, operator: &str, id: &str, fingerprint: &str) -> Result<()> {
        ensure!(operator == self.binding.account_id, "operator mismatch");
        ensure!(
            id.starts_with(&format!("{}:operator:", self.binding.session_id)),
            "operator identity mismatch"
        );
        ensure!(
            fingerprint.len() == 64
                && fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
            "invalid operator receipt digest"
        );
        Ok(())
    }

    /// Confirm the owner observed completion (success or failure) of a live invocation.
    pub async fn end(&self, request_id: &str) -> Result<()> {
        ensure!(
            !request_id.starts_with(&format!("{}:operator:", self.binding.session_id)),
            "operator completion requires a native result receipt"
        );
        sqlx::query("DELETE FROM invocations WHERE id=? AND state='live'")
            .bind(request_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Owner-only, at startup before admitting work. Live journal rows belong
    /// to a dead owner: mark them interrupted, revoke every overlay and fence
    /// generations so no grant silently covers a retry. Returns all
    /// unacknowledged interruptions.
    pub async fn recover(&self, operator: &str) -> Result<Vec<Interrupted>> {
        ensure!(
            operator == self.binding.account_id,
            "operator cannot recover this Session"
        );
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let orphaned = sqlx::query("UPDATE invocations SET state='interrupted' WHERE state='live'")
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if orphaned > 0 {
            sqlx::query("UPDATE grants SET revoked=1 WHERE revoked=0")
                .execute(&mut *tx)
                .await?;
            advance(&mut tx).await?;
        }
        let rows = sqlx::query(
            "SELECT operation, started FROM invocations WHERE state='interrupted' ORDER BY started, id",
        )
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter()
            .map(|row| {
                Ok(Interrupted {
                    operation: serde_json::from_str(&row.get::<String, _>("operation"))?,
                    started_at: u64::try_from(row.get::<i64, _>("started"))?,
                })
            })
            .collect()
    }

    /// Operator confirms interrupted effects were inspected. Records only;
    /// acknowledging never grants or replays anything.
    pub async fn acknowledge_interrupted(&self, operator: &str) -> Result<u64> {
        ensure!(
            operator == self.binding.account_id,
            "operator cannot acknowledge for this Session"
        );
        Ok(
            sqlx::query("DELETE FROM invocations WHERE state='interrupted'")
                .execute(&self.pool)
                .await?
                .rows_affected(),
        )
    }

    /// Active, unexpired Session overlays for external resources (paths,
    /// network). The owner compiles these into the executor's mounts.
    pub async fn active_resource_grants(&self, now: u64) -> Result<Vec<(Target, u64)>> {
        let rows = sqlx::query(
            "SELECT resource, expires FROM grants WHERE revoked=0 AND expires>? AND operation IS NULL AND resource IS NOT NULL ORDER BY id",
        )
        .bind(i64::try_from(now)?)
        .fetch_all(&self.pool)
        .await?;
        let mut grants = Vec::new();
        for row in rows {
            let target: Target = serde_json::from_str(&row.get::<String, _>("resource"))?;
            if matches!(
                target.capability,
                Capability::FileRead | Capability::FileWrite | Capability::Network
            ) && self.policy.mode(&target) == RuleMode::Ask
            {
                grants.push((target, u64::try_from(row.get::<i64, _>("expires"))?));
            }
        }
        Ok(grants)
    }

    /// Check before entering the provider AND before returning results/pages.
    /// An unrelated authority mutation also fences old permits deliberately.
    pub async fn check(&self, permit: &Permit, operation: &Operation, now: u64) -> Result<()> {
        self.permit_check(permit, operation, now)
            .await?
            .commit()
            .await?;
        Ok(())
    }

    async fn permit_check(
        &self,
        permit: &Permit,
        operation: &Operation,
        now: u64,
    ) -> Result<sqlx::Transaction<'_, sqlx::Sqlite>> {
        operation.validate()?;
        ensure!(
            permit.authority_fingerprint == self.fingerprint
                && permit.operation_digest == digest(&(&self.fingerprint, operation))?,
            "permit authority/operation mismatch"
        );
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let (generation, now) = clock(&mut tx, now).await?;
        let active = if let Some(id) = &permit.grant_id {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM grants WHERE id=? AND revoked=0 AND expires>?",
            )
            .bind(id)
            .bind(i64::try_from(now)?)
            .fetch_one(&mut *tx)
            .await?
                == 1
        } else {
            true
        };
        if !(generation == permit.generation && permit.expires_at > now && active) {
            // Commit the clock floor even on denial: a backward clock cannot
            // resurrect an expired grant after a failed delivery check.
            tx.commit().await?;
            anyhow::bail!("stale, revoked or expired permit");
        }
        Ok(tx)
    }
}

async fn insert_request(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    request: &Request,
) -> Result<()> {
    sqlx::query("INSERT INTO requests VALUES (?, ?, ?, 1, ?)")
        .bind(&request.operation.request_id)
        .bind(&request.digest)
        .bind(state_name(request.state))
        .bind(serde_json::to_string(request)?)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn matching_grant(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    operation: &Operation,
    now: u64,
    pinned: Option<&str>,
) -> Result<Option<(String, u64)>> {
    // A Session grant on the generic shell also covers simple prefixed commands.
    let parent = if operation.target.capability == Capability::Execute
        && operation
            .target
            .resource_id
            .starts_with(crate::ceiling::SHELL_PREFIX)
    {
        digest(&Target {
            capability: Capability::Execute,
            resource_id: crate::ceiling::WORKSPACE_SHELL.into(),
        })?
    } else {
        digest(&operation.target)?
    };
    let row = sqlx::query("SELECT id, expires FROM grants WHERE target IN (?, ?) AND revoked=0 AND expires>? AND (operation IS NULL OR operation=?) AND (? IS NULL OR id=?) ORDER BY expires DESC, id LIMIT 1")
        .bind(digest(&operation.target)?).bind(parent).bind(i64::try_from(now)?)
        .bind(&operation.request_id).bind(pinned).bind(pinned).fetch_optional(&mut **tx).await?;
    row.map(|row| {
        Ok((
            row.get::<String, _>("id"),
            u64::try_from(row.get::<i64, _>("expires"))?,
        ))
    })
    .transpose()
}

async fn clock(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, now: u64) -> Result<(i64, u64)> {
    sqlx::query("UPDATE authority SET clock=max(clock, ?) WHERE singleton=1")
        .bind(i64::try_from(now)?)
        .execute(&mut **tx)
        .await?;
    let row = sqlx::query("SELECT generation, clock FROM authority WHERE singleton=1")
        .fetch_one(&mut **tx)
        .await?;
    Ok((
        row.get("generation"),
        u64::try_from(row.get::<i64, _>("clock"))?,
    ))
}

async fn advance(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>) -> Result<()> {
    let changed = sqlx::query("UPDATE authority SET generation=generation+1 WHERE singleton=1 AND generation<9223372036854775807")
        .execute(&mut **tx).await?.rows_affected();
    ensure!(changed == 1, "permission generation exhausted");
    Ok(())
}

fn state_name(state: RequestState) -> &'static str {
    match state {
        RequestState::Pending => "pending",
        RequestState::Approved => "approved",
        RequestState::Denied => "denied",
    }
}

#[cfg(unix)]
fn prepare_private_file(path: &Path) -> Result<()> {
    use std::{
        fs::{self, OpenOptions},
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };
    let parent = path
        .parent()
        .context("permission ledger needs a private parent directory")?;
    let parent_meta = fs::symlink_metadata(parent)?;
    ensure!(
        parent_meta.is_dir()
            && !parent_meta.file_type().is_symlink()
            && parent_meta.permissions().mode() & 0o077 == 0,
        "permission directory must be private (0700)"
    );
    if !path.try_exists()? {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
        {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink() && meta.permissions().mode() & 0o077 == 0,
        "permission ledger must be private and not a symlink"
    );
    Ok(())
}

#[cfg(not(unix))]
fn prepare_private_file(_: &Path) -> Result<()> {
    anyhow::bail!("protected ledger ACL qualification is unavailable on this platform")
}
