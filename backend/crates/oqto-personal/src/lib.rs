//! Backendless host-device authorization, not a session supervisor or an Account/MFA issuer.
//! Only the local owner may call enrollment issuance/revocation. Network adapters must
//! authenticate proofs and re-check current capability grants before every action.
#![cfg(unix)]

pub mod actions;
mod grants;
pub mod http;
mod identity;
pub mod owner;
pub use identity::TransportIdentity;
/// Explicit host-owner migration of a legacy inode marker, under the authority
/// singleton lock. Preserves a private backup; does not open SQLite or issue tickets.
pub fn repair_store_owner(
    root: &std::path::Path,
    expected_marker_sha256: &str,
) -> anyhow::Result<std::path::PathBuf> {
    private::repair_owner(root, expected_marker_sha256)
}

/// Restore the exact approved legacy marker backup while no authority is active.
/// Does not alter SQLite. Restoring an obsolete mount ID restores its refusal too.
pub fn restore_store_owner(
    root: &std::path::Path,
    expected_backup_sha256: &str,
) -> anyhow::Result<()> {
    private::restore_owner(root, expected_backup_sha256)
}

mod private;
pub use private::persistent_filesystem;
#[cfg(test)]
mod private_tests;
use anyhow::{Context, Result, ensure};
pub use grants::{Access, FileGrant, Grants, SessionAccess};
use oqto_personal_wire::OQTO_PERSONAL_V1;
pub use oqto_personal_wire::{Enrollment, PairingQr, SignedRequest};
use rand::TryRngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{
    ConnectOptions, Row, Sqlite, SqlitePool, Transaction,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

/// Sensitive one-time material, intentionally not Debug. Emit only as an explicit pairing QR/output.
#[derive(Serialize)]
pub struct PairingTicket {
    pub token: String,
    pub expires_at: i64,
}

/// Opaque, non-wire context from a verified request. Not an action authorization.
#[derive(Debug)]
pub struct AuthenticatedDevice {
    id: String,
    revision: i64,
}
impl AuthenticatedDevice {
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Current capability admission held only across a bounded action/dispatch.
/// Dropping this guard lets revocation complete. Never hold it for an idle stream.
#[derive(Clone)]
pub struct DeviceFence {
    host_id: String,
    device_id: String,
    revision: i64,
    active: Arc<std::sync::atomic::AtomicBool>,
}
impl DeviceFence {
    pub fn host_id(&self) -> &str {
        &self.host_id
    }
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
    pub fn generation(&self) -> String {
        self.revision.to_string()
    }
    pub fn is_current(&self) -> bool {
        self.active.load(std::sync::atomic::Ordering::SeqCst)
    }
}

pub struct AuthorizedAction {
    grants: Grants,
    fence: DeviceFence,
    _dispatch: tokio::sync::OwnedRwLockReadGuard<()>,
}
impl AuthorizedAction {
    pub fn fence(&self) -> DeviceFence {
        self.fence.clone()
    }
    pub fn grants(&self) -> &Grants {
        &self.grants
    }
}

/// Dedicated canonical authorization store; no central backend or Account is required.
pub struct PairingAuthority {
    pool: SqlitePool,
    root: PathBuf,
    host_id: String,
    dispatch: Arc<tokio::sync::RwLock<()>>,
    fences:
        tokio::sync::Mutex<std::collections::HashMap<String, Arc<std::sync::atomic::AtomicBool>>>,
    _instance: std::fs::File,
}
impl PairingAuthority {
    pub async fn open_private(root: &Path) -> Result<Self> {
        let (root, database, instance) = private::open_state(root)?;
        let options = SqliteConnectOptions::new()
            .filename(database)
            .create_if_missing(false)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5))
            .log_statements(log::LevelFilter::Off)
            .log_slow_statements(log::LevelFilter::Off, Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        sqlx::query(
            "INSERT OR IGNORE INTO personal_host_identity (singleton, public_id) VALUES (1, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .execute(&pool)
        .await?;
        let host_id =
            sqlx::query_scalar("SELECT public_id FROM personal_host_identity WHERE singleton=1")
                .fetch_one(&pool)
                .await?;
        let checkpoint = sqlx::query("PRAGMA wal_checkpoint(FULL)")
            .fetch_one(&pool)
            .await?;
        ensure!(
            checkpoint.try_get::<i64, _>(0)? == 0,
            "personal store initialization checkpoint busy"
        );
        Ok(Self {
            pool,
            root,
            host_id,
            dispatch: Arc::new(tokio::sync::RwLock::new(())),
            fences: tokio::sync::Mutex::new(std::collections::HashMap::new()),
            _instance: instance,
        })
    }

    pub fn host_id(&self) -> &str {
        &self.host_id
    }

    /// Local-owner-only QR output using the configured HTTPS leaf certificate.
    /// This neither starts a listener nor proves its reachability.
    pub async fn issue_pairing_qr(
        &self,
        grants: &Grants,
        origin: &str,
        leaf_der: &[u8],
    ) -> Result<PairingQr> {
        oqto_personal_wire::validate_origin(origin)?;
        ensure!(
            !leaf_der.is_empty() && leaf_der.len() <= 65536,
            "invalid server certificate bounds"
        );
        let ticket = self.issue_ticket(grants).await?;
        PairingQr::new(
            &self.host_id,
            origin,
            leaf_der,
            ticket.token,
            ticket.expires_at,
            now()?,
        )
    }

    /// Untrusted transport input: bound and decode before proof/ticket admission.
    pub async fn enroll_json(&self, bytes: &[u8]) -> Result<String> {
        self.enroll(&oqto_personal_wire::decode_enrollment(bytes)?)
            .await
    }

    /// Local-owner-only issuance. Client enrollment input never supplies grants.
    pub async fn issue_ticket(&self, grants: &Grants) -> Result<PairingTicket> {
        grants.validate_selection()?;
        for file in &grants.files {
            ensure!(
                !self.root.starts_with(file.path()) && !file.path().starts_with(&self.root),
                "Files grant cannot expose personal authorization state"
            );
        }
        let mut entropy = [0u8; 32];
        rand::rngs::OsRng
            .try_fill_bytes(&mut entropy)
            .context("generate pairing secret")?;
        let token = hex::encode(entropy);
        let mut tx = self.pool.begin().await?;
        let now = clock(&mut tx, now()?).await?;
        let expires_at = now
            .checked_add(oqto_personal_wire::PAIRING_TTL_SECONDS)
            .context("pairing expiry overflow")?;
        sqlx::query("INSERT INTO personal_pairing_tickets (token_hash, grants_json, expires_at) VALUES (?, ?, ?)")
            .bind(token_hash(&token)).bind(serde_json::to_string(grants)?).bind(expires_at).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(PairingTicket { token, expires_at })
    }

    /// Atomically consume the ticket and install its selected grants after possession proof.
    pub async fn enroll(&self, enrollment: &Enrollment) -> Result<String> {
        enrollment.verify_for(OQTO_PERSONAL_V1)?;
        ensure!(
            !enrollment.name.chars().any(char::is_control),
            "device label cannot contain control characters"
        );
        let mut tx = self.pool.begin().await?;
        let now = clock(&mut tx, now()?).await?;
        let grants: Option<String> = sqlx::query_scalar("UPDATE personal_pairing_tickets SET consumed_at=? WHERE token_hash=? AND consumed_at IS NULL AND expires_at>? RETURNING grants_json")
            .bind(now).bind(token_hash(&enrollment.token)).bind(now).fetch_optional(&mut *tx).await?;
        let grants = grants.context("invalid, consumed or expired pairing ticket")?;
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO personal_devices (id, public_key, name, grants_json, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&id).bind(&enrollment.public_key).bind(&enrollment.name).bind(grants).bind(now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Consume a request nonce under the current device key; authorization is separate.
    pub async fn authenticate(&self, request: &SignedRequest) -> Result<AuthenticatedDevice> {
        self.authenticate_at(request, now()?).await
    }
    async fn authenticate_at(
        &self,
        request: &SignedRequest,
        wall_time: i64,
    ) -> Result<AuthenticatedDevice> {
        let mut tx = self.pool.begin().await?;
        // The first write serializes concurrent authentication/revocation. Persisted
        // monotonic wall time prevents old proofs reviving after nonce pruning/clock rollback.
        let effective_now = clock(&mut tx, wall_time).await?;
        let row = sqlx::query(
            "SELECT public_key, revision FROM personal_devices WHERE id=? AND revoked_at IS NULL",
        )
        .bind(&request.device_id)
        .fetch_optional(&mut *tx)
        .await?
        .context("unknown or revoked device")?;
        request.verify_for(OQTO_PERSONAL_V1, row.try_get("public_key")?, effective_now)?;
        sqlx::query("DELETE FROM personal_request_nonces WHERE accepted_at < ?")
            .bind(effective_now.saturating_sub(120))
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO personal_request_nonces (device_id, nonce, accepted_at) VALUES (?, ?, ?)",
        )
        .bind(&request.device_id)
        .bind(&request.nonce)
        .bind(effective_now)
        .execute(&mut *tx)
        .await
        .context("request nonce already consumed")?;
        let device = AuthenticatedDevice {
            id: request.device_id.clone(),
            revision: row.try_get("revision")?,
        };
        tx.commit().await?;
        Ok(device)
    }

    /// Revalidate before each bounded action/frame and retain the returned guard
    /// through dispatch. Revocation cannot complete between admission and dispatch.
    /// Adapters must separately close revoked streams and bound action duration.
    pub async fn require_current(
        &self,
        device: &AuthenticatedDevice,
        access: Access<'_>,
    ) -> Result<AuthorizedAction> {
        let dispatch = Arc::clone(&self.dispatch).read_owned().await;
        let grants: Option<String> = sqlx::query_scalar("SELECT grants_json FROM personal_devices WHERE id=? AND revision=? AND revoked_at IS NULL")
            .bind(&device.id).bind(device.revision).fetch_optional(&self.pool).await?;
        let grants: Grants =
            serde_json::from_str(&grants.context("device grant revoked or changed")?)?;
        ensure!(grants.allows(access), "device capability not granted");
        let mut fences = self.fences.lock().await;
        let flag = Arc::clone(
            fences
                .entry(device.id.clone())
                .or_insert_with(|| Arc::new(std::sync::atomic::AtomicBool::new(true))),
        );
        ensure!(
            flag.load(std::sync::atomic::Ordering::SeqCst),
            "device fence closed"
        );
        Ok(AuthorizedAction {
            grants,
            fence: DeviceFence {
                host_id: self.host_id.clone(),
                device_id: device.id.clone(),
                revision: device.revision,
                active: flag,
            },
            _dispatch: dispatch,
        })
    }

    /// Local-owner-only revocation; waits for already admitted bounded actions,
    /// then prevents further admissions. Never terminates terminal-owned work.
    pub async fn revoke(&self, id: &str) -> Result<bool> {
        let _dispatch = self.dispatch.write().await;
        // Fence FIRST, before a cancellation point in the durable write. A
        // failed/cancelled revoke stays denied in this authority's lifetime.
        let mut fences = self.fences.lock().await;
        fences
            .entry(id.to_owned())
            .or_insert_with(|| Arc::new(std::sync::atomic::AtomicBool::new(false)))
            .store(false, std::sync::atomic::Ordering::SeqCst);
        // Emergency revocation must still work if the wall clock is unavailable.
        let result = sqlx::query("UPDATE personal_devices SET revoked_at=coalesce(?, (SELECT watermark FROM personal_clock WHERE id=1)), revision=revision+1 WHERE id=? AND revoked_at IS NULL")
            .bind(now().ok()).bind(id).execute(&self.pool).await?;
        Ok(result.rows_affected() == 1)
    }
}

async fn clock(tx: &mut Transaction<'_, Sqlite>, wall: i64) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "UPDATE personal_clock SET watermark=max(watermark, ?) WHERE id=1 RETURNING watermark",
    )
    .bind(wall)
    .fetch_one(&mut **tx)
    .await?)
}
fn now() -> Result<i64> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    )?)
}
fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod bootstrap_tests;
#[cfg(test)]
mod dispatch_tests;
#[cfg(test)]
mod https_tests;
#[cfg(test)]
mod qr_tests;
#[cfg(test)]
mod tests;
