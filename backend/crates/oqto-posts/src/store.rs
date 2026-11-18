//! Durable posts, immutable idempotency receipts and reply outbox.
//! A single-connection pool serializes admission transactions across listeners.
use std::path::Path;

use anyhow::{Context, Result, ensure};
use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, Sqlite, SqliteConnection, SqlitePool, Transaction};
use uuid::Uuid;

use crate::wire::{self, Envelope, Filter, Reply, Stamp, StoredPost, WireError};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub max_posts: u32,
    pub max_age_secs: u32,
    pub posts_per_minute: u32,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            max_posts: 10_000,
            max_age_secs: 30 * 86400,
            posts_per_minute: 60,
        }
    }
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.max_posts > 0 && self.max_posts <= 1_000_000,
            "max_posts outside 1..1000000"
        );
        ensure!(
            self.max_age_secs > 0 && self.posts_per_minute > 0,
            "age and rate must be positive"
        );
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Store {
    pool: SqlitePool,
    policy: Policy,
}
#[derive(Debug)]
pub struct Page {
    pub gap: Option<i64>,
    pub through: i64,
    pub posts: Vec<(i64, StoredPost)>,
}

fn refusal(code: &str, message: &str) -> anyhow::Error {
    WireError::new(code, message).into()
}
fn fingerprint(operation: &str, value: &impl Serialize) -> Result<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&(
        operation, value,
    ))?)))
}

impl Store {
    pub async fn open(path: &Path, policy: Policy) -> Result<Self> {
        policy.validate()?;
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .context("opening post store")?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("migrating post store")?;
        Ok(Self { pool, policy })
    }
    pub async fn close(&self) {
        self.pool.close().await;
    }

    /// Idempotency keys are scoped to registration, never to a claimed agent id.
    pub async fn post(
        &self,
        identity: &str,
        stamp: &Stamp,
        request_id: &str,
        envelope: &Envelope,
        now: i64,
    ) -> Result<Value> {
        wire::id(request_id)?;
        envelope.validate()?;
        stamp.validate()?;
        let hash = fingerprint("post", envelope)?;
        let mut tx = self.pool.begin().await?;
        if let Some(result) = existing(&mut tx, identity, request_id, &hash).await? {
            return Ok(result);
        }
        let used: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM receipts WHERE identity=? AND operation='post' AND created>?",
        )
        .bind(identity)
        .bind(now - 60)
        .fetch_one(&mut *tx)
        .await?;
        if used >= i64::from(self.policy.posts_per_minute) {
            return Err(refusal("rate_limited", "identity post rate exceeded"));
        }
        let mut effective = envelope.clone();
        stamp.label(&mut effective);
        effective.validate().map_err(|_| {
            refusal(
                "invalid_request",
                "effective envelope exceeds bounds; reserve one label slot for Runner route",
            )
        })?;
        let stored = StoredPost {
            id: Uuid::new_v4().to_string(),
            stored_at: Utc
                .timestamp_opt(now, 0)
                .single()
                .context("invalid clock")?
                .to_rfc3339(),
            stamped: stamp.clone(),
            envelope: effective,
        };
        sqlx::query("INSERT INTO posts(post_id,identity,created,payload) VALUES(?,?,?,?)")
            .bind(&stored.id)
            .bind(identity)
            .bind(now)
            .bind(serde_json::to_string(&stored)?)
            .execute(&mut *tx)
            .await?;
        let result = json!({"post_id":stored.id,"stored_at":stored.stored_at,"labels":stored.envelope.labels});
        receipt(&mut tx, identity, request_id, "post", &hash, now, &result).await?;
        self.prune(&mut tx, now).await?;
        tx.commit().await.context("committing post")?;
        Ok(result)
    }

    /// Admit a reply and its outbox record atomically. No Session input is
    /// performed here. A sink must use request_id for durable deduplication.
    pub async fn reply(
        &self,
        identity: &str,
        request_id: &str,
        reply: &Reply,
        now: i64,
    ) -> Result<Value> {
        wire::id(request_id)?;
        reply.validate()?;
        let hash = fingerprint("reply", reply)?;
        let mut tx = self.pool.begin().await?;
        if let Some(result) = existing(&mut tx, identity, request_id, &hash).await? {
            return Ok(result);
        }
        let payload: Option<String> =
            sqlx::query_scalar("SELECT payload FROM posts WHERE post_id=?")
                .bind(&reply.post_id)
                .fetch_optional(&mut *tx)
                .await?;
        let stored: StoredPost = serde_json::from_str(
            &payload.ok_or_else(|| refusal("not_found", "post absent or expired"))?,
        )?;
        if reply.mode == wire::Mode::NewTask {
            return Err(refusal("invalid_request", "new-task unsupported in v1"));
        }
        let result = json!({"reply_id":Uuid::new_v4().to_string(),"post_id":reply.post_id,"mode":reply.mode,"status":"queued"});
        receipt(&mut tx, identity, request_id, "reply", &hash, now, &result).await?;
        sqlx::query("INSERT INTO reply_outbox(identity,request_id,target,reply) VALUES(?,?,?,?)")
            .bind(identity)
            .bind(request_id)
            .bind(serde_json::to_string(&stored.stamped)?)
            .bind(serde_json::to_string(reply)?)
            .execute(&mut *tx)
            .await?;
        tx.commit().await.context("committing reply admission")?;
        Ok(result)
    }

    /// Explicitly replay a bounded outbox batch. Nothing retries in the
    /// background. A Session sink must deduplicate the unchanged request id;
    /// cancellation between sink acceptance and this commit leaves it pending.
    pub async fn dispatch_pending(
        &self,
        sink: &dyn crate::reply_sink::ReplySink,
    ) -> Result<crate::reply_sink::DispatchReport> {
        use crate::reply_sink::{DeliveryRequest, DispatchReport};
        let rows=sqlx::query("SELECT identity,request_id,target,reply FROM reply_outbox WHERE delivered=0 ORDER BY rowid LIMIT 128")
            .fetch_all(&self.pool).await.context("reading reply outbox")?;
        let mut report = DispatchReport::default();
        for row in rows {
            let request = DeliveryRequest {
                owner_identity: row.try_get("identity")?,
                request_id: row.try_get("request_id")?,
                target: serde_json::from_str(row.try_get::<&str, _>("target")?)?,
                reply: serde_json::from_str(row.try_get::<&str, _>("reply")?)?,
            };
            let receipt = match sink.deliver(&request).await {
                Ok(receipt) => receipt,
                Err(_) => {
                    report.pending += 1;
                    continue;
                }
            };
            receipt.validate(request.reply.mode)?;
            let mut tx = self.pool.begin().await?;
            let stored: String =
                sqlx::query_scalar("SELECT result FROM receipts WHERE identity=? AND request_id=?")
                    .bind(&request.owner_identity)
                    .bind(&request.request_id)
                    .fetch_one(&mut *tx)
                    .await?;
            let mut result: Value = serde_json::from_str(&stored)?;
            result["status"] = serde_json::to_value(receipt.status)?;
            if receipt.effective_mode != request.reply.mode {
                result["effective_mode"] = serde_json::to_value(receipt.effective_mode)?;
            }
            sqlx::query("UPDATE receipts SET result=? WHERE identity=? AND request_id=?")
                .bind(serde_json::to_string(&result)?)
                .bind(&request.owner_identity)
                .bind(&request.request_id)
                .execute(&mut *tx)
                .await?;
            // The durable receipt retains the id/hash/ack, not delivered text.
            sqlx::query("DELETE FROM reply_outbox WHERE identity=? AND request_id=?")
                .bind(&request.owner_identity)
                .bind(&request.request_id)
                .execute(&mut *tx)
                .await?;
            tx.commit()
                .await
                .context("committing reply delivery receipt")?;
            report.accepted += 1;
            // Queued means the sink accepted it durably, so it owns delivery
            // now too. Neither status requires another outbox dispatch.
        }
        Ok(report)
    }

    /// A short replay page, followed by polling from `through`. The high-water
    /// mark advances over filtered records too. Snapshot and retention floor
    /// are read in the same transaction, so no append can fall through a gap.
    pub async fn page(&self, since: i64, filter: &Filter, now: i64) -> Result<Page> {
        let mut tx = self.pool.begin().await?;
        self.prune(&mut tx, now).await?;
        let floor: i64 =
            sqlx::query_scalar("SELECT value FROM store_meta WHERE key='pruned_through'")
                .fetch_one(&mut *tx)
                .await?;
        let rows = sqlx::query("SELECT seq,payload FROM posts WHERE seq>? ORDER BY seq LIMIT 128")
            .bind(since.max(floor))
            .fetch_all(&mut *tx)
            .await?;
        let mut through = since.max(floor);
        let mut posts = Vec::new();
        for row in rows {
            through = row.try_get("seq")?;
            let post: StoredPost = serde_json::from_str(row.try_get::<&str, _>("payload")?)?;
            if filter.matches(&post) {
                posts.push((through, post));
            }
        }
        tx.commit().await?;
        Ok(Page {
            gap: (since < floor).then_some(floor),
            through,
            posts,
        })
    }

    async fn prune(&self, tx: &mut Transaction<'_, Sqlite>, now: i64) -> Result<()> {
        let boundary: Option<i64> =
            sqlx::query_scalar("SELECT seq FROM posts ORDER BY seq DESC LIMIT 1 OFFSET ?")
                .bind(i64::from(self.policy.max_posts))
                .fetch_optional(&mut **tx)
                .await?;
        let floor: Option<i64> =
            sqlx::query_scalar("SELECT MAX(seq) FROM posts WHERE created<? OR seq<=?")
                .bind(now - i64::from(self.policy.max_age_secs))
                .bind(boundary.unwrap_or(0))
                .fetch_one(&mut **tx)
                .await?;
        if let Some(floor) = floor {
            sqlx::query("UPDATE store_meta SET value=MAX(value,?) WHERE key='pruned_through'")
                .bind(floor)
                .execute(&mut **tx)
                .await?;
            sqlx::query("DELETE FROM posts WHERE seq<=?")
                .bind(floor)
                .execute(&mut **tx)
                .await?;
        }
        Ok(())
    }
}

async fn existing(
    connection: &mut SqliteConnection,
    identity: &str,
    request_id: &str,
    hash: &str,
) -> Result<Option<Value>> {
    let row =
        sqlx::query("SELECT fingerprint,result FROM receipts WHERE identity=? AND request_id=?")
            .bind(identity)
            .bind(request_id)
            .fetch_optional(connection)
            .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.try_get::<&str, _>("fingerprint")? != hash {
        return Err(refusal(
            "invalid_request",
            "request_id reused with different content",
        ));
    }
    let mut result: Value = serde_json::from_str(row.try_get::<&str, _>("result")?)?;
    result["duplicate"] = json!(true);
    Ok(Some(result))
}
async fn receipt(
    connection: &mut SqliteConnection,
    identity: &str,
    request_id: &str,
    operation: &str,
    hash: &str,
    now: i64,
    result: &Value,
) -> Result<()> {
    sqlx::query("INSERT INTO receipts(identity,request_id,operation,fingerprint,created,result) VALUES(?,?,?,?,?,?)")
        .bind(identity).bind(request_id).bind(operation).bind(hash).bind(now).bind(serde_json::to_string(result)?).execute(connection).await?;
    Ok(())
}

pub fn cursor(sequence: i64) -> String {
    format!("c-{sequence}")
}
pub fn sequence(cursor: &str) -> Result<i64> {
    let sequence: i64 = cursor
        .strip_prefix("c-")
        .context("invalid cursor prefix")?
        .parse()
        .context("invalid cursor sequence")?;
    ensure!(sequence >= 0, "negative cursor");
    Ok(sequence)
}
