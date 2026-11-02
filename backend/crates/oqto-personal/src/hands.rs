//! Destination-owned, single-command consent and durable no-replay receipts.
//! Caller labels are attribution supplied by an authenticated device, not proof
//! of an Oqto Session identity. Only the private owner API can approve a command.
use crate::{DeviceFence, FileGrant, PairingAuthority, now};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct HandsReceipt {
    pub id: String,
    pub state: String,
    pub output: Option<String>,
    pub exit_code: Option<i32>,
    /// Cancellation/restart cannot establish that earlier effects did not occur.
    pub effects_uncertain: bool,
}

pub struct HandsClaim {
    pub id: String,
    pub command: String,
    pub root: FileGrant,
    pub expires_at: i64,
    pub fence: DeviceFence,
}

impl PairingAuthority {
    async fn hands_clock(&self) -> Result<i64> {
        let mut tx = self.pool.begin().await?;
        let wall = crate::clock(&mut tx, now()?).await?;
        tx.commit().await?;
        Ok(wall)
    }

    pub async fn uncertain_hands(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE personal_hands SET state='uncertain',output=NULL WHERE id=? AND state='running'")
            .bind(id).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn request_hands(
        &self,
        device: &str,
        key: &str,
        session: &str,
        command: &str,
    ) -> Result<HandsReceipt> {
        ensure!(
            !key.is_empty() && key.len() <= 128,
            "invalid invocation key"
        );
        ensure!(
            !session.is_empty() && session.len() <= 128,
            "invalid Session label"
        );
        ensure!(
            !command.is_empty() && command.len() <= 8192 && !command.contains('\0'),
            "invalid command"
        );
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT OR IGNORE INTO personal_hands(id,device_id,request_key,session_label,command,state,expires_at) SELECT ?,?,?,?,?,'pending',? WHERE (SELECT count(*) FROM personal_hands WHERE device_id=?)<1024")
            .bind(id).bind(device).bind(key).bind(session).bind(command).bind(self.hands_clock().await? + 300).bind(device).execute(&self.pool).await?;
        let row = sqlx::query("SELECT id,session_label,command FROM personal_hands WHERE device_id=? AND request_key=?")
            .bind(device).bind(key).fetch_one(&self.pool).await?;
        ensure!(
            row.try_get::<String, _>("session_label")? == session
                && row.try_get::<String, _>("command")? == command,
            "invocation key reused with different input"
        );
        self.hands_receipt(device, &row.try_get::<String, _>("id")?)
            .await
    }

    pub async fn hands_receipt(&self, device: &str, id: &str) -> Result<HandsReceipt> {
        sqlx::query("UPDATE personal_hands SET state='denied' WHERE id=? AND device_id=? AND state IN ('pending','approved') AND expires_at<=?")
            .bind(id).bind(device).bind(self.hands_clock().await?).execute(&self.pool).await?;
        let row = sqlx::query(
            "SELECT id,state,output,exit_code FROM personal_hands WHERE id=? AND device_id=?",
        )
        .bind(id)
        .bind(device)
        .fetch_one(&self.pool)
        .await?;
        let state: String = row.try_get("state")?;
        let effects_uncertain = matches!(state.as_str(), "running" | "uncertain" | "cancelled");
        Ok(HandsReceipt {
            effects_uncertain,
            id: row.try_get("id")?,
            state,
            output: row.try_get("output")?,
            exit_code: row.try_get("exit_code")?,
        })
    }

    /// Owner-only: includes command text for exact inspection. Never send this
    /// listing to another device or render it without terminal escaping.
    pub async fn hands_requests(&self) -> Result<serde_json::Value> {
        let rows = sqlx::query("SELECT h.id,h.device_id,h.session_label,h.command,h.state,h.expires_at,d.name FROM personal_hands h JOIN personal_devices d ON d.id=h.device_id ORDER BY h.rowid DESC LIMIT 64")
            .fetch_all(&self.pool).await?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::json!({"id":row.try_get::<String,_>("id")?,"device_id":row.try_get::<String,_>("device_id")?,"device_name":row.try_get::<String,_>("name")?,"session_label_unverified":row.try_get::<String,_>("session_label")?,"command":row.try_get::<String,_>("command")?,"state":row.try_get::<String,_>("state")?,"expires_at":row.try_get::<i64,_>("expires_at")?}));
        }
        Ok(serde_json::json!({"requests":out}))
    }

    /// A specific invocation, root and expiry are approved. Files grants never
    /// imply shell authority. Denial/cancellation cannot be reversed.
    pub async fn decide_hands(&self, id: &str, root: Option<&Path>) -> Result<bool> {
        let wall = self.hands_clock().await?;
        let (state, root) = match root {
            Some(path) => {
                let root = FileGrant::select(path, true)?;
                ensure!(
                    !self.root.starts_with(root.path()),
                    "execution root contains Runner authority"
                );
                ("approved", Some(serde_json::to_string(&root)?))
            }
            None => ("denied", None),
        };
        let result = sqlx::query("UPDATE personal_hands SET state=?,root_json=?,expires_at=? WHERE id=? AND state='pending' AND expires_at>? AND device_id IN (SELECT id FROM personal_devices WHERE revoked_at IS NULL)")
            .bind(state).bind(root).bind(wall + 30).bind(id).bind(wall).execute(&self.pool).await?;
        Ok(result.rows_affected() == 1)
    }

    /// Atomically claim once BEFORE spawning. A lost response or process crash
    /// never makes this invocation runnable a second time.
    pub async fn claim_hands(
        &self,
        device: &str,
        id: &str,
        fence: DeviceFence,
    ) -> Result<Option<HandsClaim>> {
        ensure!(
            fence.is_current() && fence.device_id() == device,
            "device revoked"
        );
        let row = sqlx::query("UPDATE personal_hands SET state='running' WHERE id=? AND device_id=? AND state='approved' AND expires_at>? AND (SELECT count(*) FROM personal_hands WHERE state='running')<4 RETURNING command,root_json,expires_at")
            .bind(id).bind(device).bind(self.hands_clock().await?).fetch_optional(&self.pool).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let root: FileGrant = serde_json::from_str(&row.try_get::<String, _>("root_json")?)?;
        if let Err(error) = root.validate_identity() {
            self.uncertain_hands(id).await?;
            return Err(error).context("approved root replaced; invocation will not replay");
        }
        Ok(Some(HandsClaim {
            id: id.into(),
            command: row.try_get("command")?,
            root,
            expires_at: row.try_get("expires_at")?,
            fence,
        }))
    }

    pub async fn hands_active(&self, claim: &HandsClaim) -> Result<bool> {
        if !claim.fence.is_current() || self.hands_clock().await? >= claim.expires_at {
            return Ok(false);
        }
        let state: Option<String> =
            sqlx::query_scalar("SELECT state FROM personal_hands WHERE id=?")
                .bind(&claim.id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(state.as_deref() == Some("running"))
    }

    pub async fn cancel_hands(&self, device: Option<&str>, id: &str) -> Result<bool> {
        let result = sqlx::query("UPDATE personal_hands SET state='cancelled',output=NULL WHERE id=? AND (? IS NULL OR device_id=?) AND state IN ('pending','approved','running')")
            .bind(id).bind(device).bind(device).execute(&self.pool).await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn finish_hands(
        &self,
        claim: &HandsClaim,
        output: &str,
        code: Option<i32>,
    ) -> Result<()> {
        ensure!(output.len() <= 65536, "output too large");
        let active = self.hands_active(claim).await?;
        sqlx::query(
            "UPDATE personal_hands SET state=?,output=?,exit_code=? WHERE id=? AND state='running'",
        )
        .bind(if active { "done" } else { "uncertain" })
        .bind(if active { Some(output) } else { None })
        .bind(code)
        .bind(&claim.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
