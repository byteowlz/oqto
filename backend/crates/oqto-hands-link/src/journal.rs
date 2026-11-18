//! Destination dispatch journal: the authority for claims, controller
//! takeover, fences and stop receipts (DESIGN.md sections 5, 7, 8). Not a
//! transcript store. Every admission check and the claim insert happen in one
//! transaction, committed before the claim is acknowledged or any frame is
//! forwarded.
use crate::wire::{CallState, Operation, Receipt, Refusal, StopState};
use anyhow::{Context, Result, ensure};
use sqlx::{
    ConnectOptions, Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{path::Path, time::Duration};

pub struct Journal {
    pool: SqlitePool,
}

pub struct NewClaim<'a> {
    pub call_id: &'a str,
    pub device_id: &'a str,
    pub namespace: &'a str,
    pub controller_epoch: u64,
    pub binding_id: &'a str,
    pub policy_revision: u64,
    pub digest: &'a str,
    pub operation: Operation,
    pub cwd: &'a str,
    pub host_boot: &'a str,
}

#[derive(Debug)]
pub enum ClaimOutcome {
    Fresh {
        generation: u64,
    },
    /// Exact duplicate: the stored receipt, never a second dispatch.
    Existing(Receipt),
    Refused(Refusal),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Activation {
    /// First activation or an authorized takeover of `previous`.
    Activated {
        previous: Option<u64>,
    },
    /// Same controller epoch reconnecting with a fresh control epoch.
    Resumed,
    Refused,
}

#[derive(Debug, Default)]
pub struct Recovery {
    /// Claims a dead host left claimed or running: now unknown, never replayed.
    pub unknown: Vec<(String, u64)>,
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn int(value: u64) -> Result<i64> {
    i64::try_from(value).context("value exceeds journal range")
}

impl Journal {
    pub async fn open(path: &Path) -> Result<(Self, Recovery)> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .busy_timeout(Duration::from_secs(5))
            .log_statements(log::LevelFilter::Off);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .context("opening hands journal")?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        let rows = sqlx::query(
            "UPDATE hands_claims SET state='unknown', updated_at=? WHERE state IN ('claimed','running') RETURNING call_id, generation",
        )
        .bind(now())
        .fetch_all(&pool)
        .await?;
        let mut recovery = Recovery::default();
        for row in rows {
            recovery.unknown.push((
                row.try_get("call_id")?,
                u64::try_from(row.try_get::<i64, _>("generation")?)?,
            ));
        }
        Ok((Self { pool }, recovery))
    }

    pub async fn delegate(&self, device_id: &str, binding_id: &str) -> Result<bool> {
        let done = sqlx::query(
            "INSERT OR IGNORE INTO hands_delegations (device_id, binding_id, created_at) VALUES (?, ?, ?)",
        )
        .bind(device_id)
        .bind(binding_id)
        .bind(now())
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn undelegate(&self, device_id: &str, binding_id: &str) -> Result<bool> {
        let done = sqlx::query("DELETE FROM hands_delegations WHERE device_id=? AND binding_id=?")
            .bind(device_id)
            .bind(binding_id)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn delegated(&self, device_id: &str, binding_id: &str) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM hands_delegations WHERE device_id=? AND binding_id=?",
        )
        .bind(device_id)
        .bind(binding_id)
        .fetch_one(&self.pool)
        .await?
            == 1)
    }

    pub async fn delegations(&self) -> Result<Vec<(String, String)>> {
        let rows = sqlx::query(
            "SELECT device_id, binding_id FROM hands_delegations ORDER BY device_id, binding_id LIMIT 256",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| Ok((row.try_get("device_id")?, row.try_get("binding_id")?)))
            .collect()
    }

    /// Compare-and-swap controller activation. No newest-connection-wins:
    /// a takeover must name the stored epoch as its predecessor.
    pub async fn activate_controller(
        &self,
        device_id: &str,
        namespace: &str,
        predecessor: u64,
        epoch: u64,
        live: bool,
    ) -> Result<Activation> {
        let mut tx = self.pool.begin().await?;
        let stored: Option<i64> = sqlx::query_scalar(
            "SELECT epoch FROM hands_controllers WHERE device_id=? AND namespace=?",
        )
        .bind(device_id)
        .bind(namespace)
        .fetch_optional(&mut *tx)
        .await?;
        let stored = stored.map(u64::try_from).transpose()?;
        let activation = match stored {
            Some(current) if current == epoch && !live => Activation::Resumed,
            Some(current) if current == predecessor && epoch == predecessor + 1 => {
                Activation::Activated {
                    previous: Some(current),
                }
            }
            None if predecessor == 0 && epoch == 1 => Activation::Activated { previous: None },
            _ => Activation::Refused,
        };
        if let Activation::Activated { .. } = activation {
            sqlx::query(
                "INSERT INTO hands_controllers (device_id, namespace, epoch) VALUES (?, ?, ?) ON CONFLICT(device_id, namespace) DO UPDATE SET epoch=excluded.epoch",
            )
            .bind(device_id)
            .bind(namespace)
            .bind(int(epoch)?)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(activation)
    }

    pub async fn controller_current(
        &self,
        device_id: &str,
        namespace: &str,
        epoch: u64,
    ) -> Result<bool> {
        let stored: Option<i64> = sqlx::query_scalar(
            "SELECT epoch FROM hands_controllers WHERE device_id=? AND namespace=?",
        )
        .bind(device_id)
        .bind(namespace)
        .fetch_optional(&self.pool)
        .await?;
        Ok(stored == Some(int(epoch)?))
    }

    /// Persist an origin fence. Returns false for an older revision.
    pub async fn raise_fence(
        &self,
        device_id: &str,
        binding_id: &str,
        revision: u64,
    ) -> Result<bool> {
        let done = sqlx::query(
            "INSERT INTO hands_fences (device_id, binding_id, policy_revision) VALUES (?, ?, ?) ON CONFLICT(device_id, binding_id) DO UPDATE SET policy_revision=excluded.policy_revision WHERE excluded.policy_revision >= hands_fences.policy_revision",
        )
        .bind(device_id)
        .bind(binding_id)
        .bind(int(revision)?)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn fence(&self, device_id: &str, binding_id: &str) -> Result<u64> {
        let stored: Option<i64> = sqlx::query_scalar(
            "SELECT policy_revision FROM hands_fences WHERE device_id=? AND binding_id=?",
        )
        .bind(device_id)
        .bind(binding_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(stored.map(u64::try_from).transpose()?.unwrap_or(0))
    }

    /// One transaction: duplicate inspection, delegation, controller, fence,
    /// attention, then the immutable claim with a fresh generation.
    pub async fn claim(&self, claim: &NewClaim<'_>) -> Result<ClaimOutcome> {
        let mut tx = self.pool.begin().await?;
        if let Some(row) = sqlx::query("SELECT * FROM hands_claims WHERE call_id=?")
            .bind(claim.call_id)
            .fetch_optional(&mut *tx)
            .await?
        {
            let receipt = receipt(&row)?;
            let same_owner: String = row.try_get("device_id")?;
            return Ok(
                if receipt.digest == claim.digest && same_owner == claim.device_id {
                    ClaimOutcome::Existing(receipt)
                } else {
                    ClaimOutcome::Refused(Refusal::DigestMismatch)
                },
            );
        }
        let delegated: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM hands_delegations WHERE device_id=? AND binding_id=?",
        )
        .bind(claim.device_id)
        .bind(claim.binding_id)
        .fetch_one(&mut *tx)
        .await?;
        if delegated != 1 {
            return Ok(ClaimOutcome::Refused(Refusal::NotDelegated));
        }
        let controller: Option<i64> = sqlx::query_scalar(
            "SELECT epoch FROM hands_controllers WHERE device_id=? AND namespace=?",
        )
        .bind(claim.device_id)
        .bind(claim.namespace)
        .fetch_optional(&mut *tx)
        .await?;
        if controller != Some(int(claim.controller_epoch)?) {
            return Ok(ClaimOutcome::Refused(Refusal::StaleController));
        }
        let fence: Option<i64> = sqlx::query_scalar(
            "SELECT policy_revision FROM hands_fences WHERE device_id=? AND binding_id=?",
        )
        .bind(claim.device_id)
        .bind(claim.binding_id)
        .fetch_optional(&mut *tx)
        .await?;
        if fence.is_some_and(|fence| int(claim.policy_revision).is_ok_and(|rev| rev < fence)) {
            return Ok(ClaimOutcome::Refused(Refusal::StalePolicy));
        }
        let attention: i64 =
            sqlx::query_scalar("SELECT count(*) FROM hands_attention WHERE binding_id=?")
                .bind(claim.binding_id)
                .fetch_one(&mut *tx)
                .await?;
        if attention != 0 {
            return Ok(ClaimOutcome::Refused(Refusal::Attention));
        }
        let generation: i64 = sqlx::query_scalar(
            "UPDATE hands_meta SET next_generation=next_generation+1 WHERE singleton=1 RETURNING next_generation-1",
        )
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO hands_claims (call_id, device_id, namespace, controller_epoch, binding_id, policy_revision, digest, operation, cwd, generation, host_boot, state, stop, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'claimed', 'not_started', ?, ?)",
        )
        .bind(claim.call_id)
        .bind(claim.device_id)
        .bind(claim.namespace)
        .bind(int(claim.controller_epoch)?)
        .bind(claim.binding_id)
        .bind(int(claim.policy_revision)?)
        .bind(claim.digest)
        .bind(operation_name(claim.operation))
        .bind(claim.cwd)
        .bind(generation)
        .bind(claim.host_boot)
        .bind(now())
        .bind(now())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(ClaimOutcome::Fresh {
            generation: u64::try_from(generation)?,
        })
    }

    /// Record a state transition. Terminal states never change again.
    pub async fn transition(
        &self,
        call_id: &str,
        state: CallState,
        stop: StopState,
        bytes: Option<(u64, u64)>,
    ) -> Result<Receipt> {
        let (bytes_in, bytes_out) = match bytes {
            Some((a, b)) => (Some(int(a)?), Some(int(b)?)),
            None => (None, None),
        };
        let row = sqlx::query(
            "UPDATE hands_claims SET state=?, stop=?, bytes_in=coalesce(?, bytes_in), bytes_out=coalesce(?, bytes_out), updated_at=? WHERE call_id=? AND state IN ('claimed','running') RETURNING *",
        )
        .bind(state.as_str())
        .bind(stop.as_str())
        .bind(bytes_in)
        .bind(bytes_out)
        .bind(now())
        .bind(call_id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => receipt(&row),
            None => self
                .receipt(call_id)
                .await?
                .context("no claim for this call"),
        }
    }

    /// Stop evidence may be completed after the terminal state was recorded.
    pub async fn record_stop(&self, call_id: &str, stop: StopState) -> Result<Receipt> {
        sqlx::query("UPDATE hands_claims SET stop=?, updated_at=? WHERE call_id=? AND stop IN ('pending','not_started')")
            .bind(stop.as_str())
            .bind(now())
            .bind(call_id)
            .execute(&self.pool)
            .await?;
        self.receipt(call_id)
            .await?
            .context("no claim for this call")
    }

    pub async fn receipt(&self, call_id: &str) -> Result<Option<Receipt>> {
        sqlx::query("SELECT * FROM hands_claims WHERE call_id=?")
            .bind(call_id)
            .fetch_optional(&self.pool)
            .await?
            .map(|row| receipt(&row))
            .transpose()
    }

    pub async fn claim_owner(&self, call_id: &str) -> Result<Option<(String, String)>> {
        let row = sqlx::query("SELECT device_id, namespace FROM hands_claims WHERE call_id=?")
            .bind(call_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| Ok((row.try_get("device_id")?, row.try_get("namespace")?)))
            .transpose()
    }

    pub async fn set_attention(&self, binding_id: &str, reason: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO hands_attention (binding_id, reason, at) VALUES (?, ?, ?) ON CONFLICT(binding_id) DO NOTHING",
        )
        .bind(binding_id)
        .bind(reason)
        .bind(now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn clear_attention(&self, binding_id: &str) -> Result<bool> {
        let done = sqlx::query("DELETE FROM hands_attention WHERE binding_id=?")
            .bind(binding_id)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn attention(&self) -> Result<Vec<(String, String)>> {
        let rows =
            sqlx::query("SELECT binding_id, reason FROM hands_attention ORDER BY binding_id")
                .fetch_all(&self.pool)
                .await?;
        rows.iter()
            .map(|row| Ok((row.try_get("binding_id")?, row.try_get("reason")?)))
            .collect()
    }

    pub async fn recent(&self, limit: u32) -> Result<Vec<Receipt>> {
        ensure!(limit <= 256, "limit too large");
        let rows = sqlx::query("SELECT * FROM hands_claims ORDER BY generation DESC LIMIT ?")
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(receipt).collect()
    }
}

fn operation_name(operation: Operation) -> &'static str {
    match operation {
        Operation::Read => "read",
        Operation::Write => "write",
        Operation::Edit => "edit",
        Operation::Bash => "bash",
    }
}

fn parse_operation(value: &str) -> Result<Operation> {
    Ok(match value {
        "read" => Operation::Read,
        "write" => Operation::Write,
        "edit" => Operation::Edit,
        "bash" => Operation::Bash,
        _ => anyhow::bail!("invalid stored operation"),
    })
}

fn receipt(row: &sqlx::sqlite::SqliteRow) -> Result<Receipt> {
    Ok(Receipt {
        call_id: row.try_get("call_id")?,
        binding_id: row.try_get("binding_id")?,
        generation: u64::try_from(row.try_get::<i64, _>("generation")?)?,
        host_boot: row.try_get("host_boot")?,
        digest: row.try_get("digest")?,
        operation: parse_operation(&row.try_get::<String, _>("operation")?)?,
        state: CallState::parse(&row.try_get::<String, _>("state")?)?,
        stop: StopState::parse(&row.try_get::<String, _>("stop")?)?,
        cwd: row.try_get("cwd")?,
        bytes_in: u64::try_from(row.try_get::<i64, _>("bytes_in")?)?,
        bytes_out: u64::try_from(row.try_get::<i64, _>("bytes_out")?)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim<'a>(call: &'a str, digest: &'a str, epoch: u64, revision: u64) -> NewClaim<'a> {
        NewClaim {
            call_id: call,
            device_id: "dev",
            namespace: "ns",
            controller_epoch: epoch,
            binding_id: "workspace",
            policy_revision: revision,
            digest,
            operation: Operation::Bash,
            cwd: "/w",
            host_boot: "boot",
        }
    }

    #[tokio::test]
    async fn claims_are_immutable_and_restart_marks_unknown() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("hands.sqlite3");
        let (journal, _) = Journal::open(&path).await?;
        assert!(matches!(
            journal.claim(&claim("c1", "d1", 1, 0)).await?,
            ClaimOutcome::Refused(Refusal::NotDelegated)
        ));
        journal.delegate("dev", "workspace").await?;
        assert!(matches!(
            journal.claim(&claim("c1", "d1", 1, 0)).await?,
            ClaimOutcome::Refused(Refusal::StaleController)
        ));
        assert_eq!(
            journal
                .activate_controller("dev", "ns", 0, 1, false)
                .await?,
            Activation::Activated { previous: None }
        );
        let ClaimOutcome::Fresh { generation } = journal.claim(&claim("c1", "d1", 1, 0)).await?
        else {
            anyhow::bail!("expected a fresh claim");
        };
        // Duplicate inspects; changed digest is refused.
        assert!(matches!(
            journal.claim(&claim("c1", "d1", 1, 0)).await?,
            ClaimOutcome::Existing(ref r) if r.generation == generation && r.state == CallState::Claimed
        ));
        assert!(matches!(
            journal.claim(&claim("c1", "d2", 1, 0)).await?,
            ClaimOutcome::Refused(Refusal::DigestMismatch)
        ));
        journal
            .transition("c1", CallState::Running, StopState::Pending, None)
            .await?;
        drop(journal);
        let (journal, recovery) = Journal::open(&path).await?;
        assert_eq!(recovery.unknown, vec![("c1".to_string(), generation)]);
        let receipt = journal.receipt("c1").await?.context("receipt")?;
        assert_eq!(receipt.state, CallState::Unknown);
        // Restart never resurrects it, and the duplicate still only inspects.
        assert!(matches!(
            journal.claim(&claim("c1", "d1", 1, 0)).await?,
            ClaimOutcome::Existing(ref r) if r.state == CallState::Unknown
        ));
        // Terminal states are final.
        let after = journal
            .transition("c1", CallState::Released, StopState::Confirmed, None)
            .await?;
        assert_eq!(after.state, CallState::Unknown);
        Ok(())
    }

    #[tokio::test]
    async fn controller_cas_fences_and_attention() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let (journal, _) = Journal::open(&dir.path().join("j.sqlite3")).await?;
        journal.delegate("dev", "workspace").await?;
        assert_eq!(
            journal
                .activate_controller("dev", "ns", 0, 1, false)
                .await?,
            Activation::Activated { previous: None }
        );
        // A live duplicate of the same epoch and a racing guard are refused.
        assert_eq!(
            journal.activate_controller("dev", "ns", 0, 1, true).await?,
            Activation::Refused
        );
        assert_eq!(
            journal.activate_controller("dev", "ns", 1, 2, true).await?,
            Activation::Activated { previous: Some(1) }
        );
        assert_eq!(
            journal.activate_controller("dev", "ns", 1, 2, true).await?,
            Activation::Refused
        );
        assert_eq!(
            journal
                .activate_controller("dev", "ns", 0, 1, false)
                .await?,
            Activation::Refused
        );
        assert_eq!(
            journal
                .activate_controller("dev", "ns", 2, 2, false)
                .await?,
            Activation::Resumed
        );
        assert!(matches!(
            journal.claim(&claim("old", "d", 1, 0)).await?,
            ClaimOutcome::Refused(Refusal::StaleController)
        ));
        assert!(journal.raise_fence("dev", "workspace", 3).await?);
        assert!(!journal.raise_fence("dev", "workspace", 2).await?);
        assert!(matches!(
            journal.claim(&claim("c", "d", 2, 2)).await?,
            ClaimOutcome::Refused(Refusal::StalePolicy)
        ));
        journal
            .set_attention("workspace", "stop unconfirmed")
            .await?;
        assert!(matches!(
            journal.claim(&claim("c", "d", 2, 3)).await?,
            ClaimOutcome::Refused(Refusal::Attention)
        ));
        assert!(journal.clear_attention("workspace").await?);
        assert!(matches!(
            journal.claim(&claim("c", "d", 2, 3)).await?,
            ClaimOutcome::Fresh { .. }
        ));
        Ok(())
    }
}
