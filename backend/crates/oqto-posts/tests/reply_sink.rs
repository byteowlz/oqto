use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use oqto_posts::{
    reply_sink::{DeliveryReceipt, DeliveryRequest, DeliveryStatus, ReplySink},
    store::{Policy, Store},
    wire::{Envelope, Mode, Reply, Route, Stamp},
};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};

/// Synthetic durable Session acceptance store, never a real Session or Pi file.
struct TestSink {
    pool: sqlx::SqlitePool,
    fail_after_commit: AtomicBool,
}
impl TestSink {
    async fn open(path: &std::path::Path, fail: bool) -> Result<Self> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true),
            )
            .await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS accepted(session TEXT,request_id TEXT,body TEXT,PRIMARY KEY(session,request_id))").execute(&pool).await?;
        Ok(Self {
            pool,
            fail_after_commit: AtomicBool::new(fail),
        })
    }
}
#[async_trait]
impl ReplySink for TestSink {
    async fn deliver(&self, request: &DeliveryRequest) -> Result<DeliveryReceipt> {
        let session = request
            .target
            .session
            .as_deref()
            .context("no public Session id")?;
        let body = serde_json::to_string(&request.reply)?;
        sqlx::query("INSERT OR IGNORE INTO accepted VALUES(?,?,?)")
            .bind(session)
            .bind(&request.request_id)
            .bind(&body)
            .execute(&self.pool)
            .await?;
        let stored: String =
            sqlx::query_scalar("SELECT body FROM accepted WHERE session=? AND request_id=?")
                .bind(session)
                .bind(&request.request_id)
                .fetch_one(&self.pool)
                .await?;
        ensure!(stored == body, "Session idempotency conflict");
        if self.fail_after_commit.swap(false, Ordering::SeqCst) {
            anyhow::bail!("synthetic dropped acceptance reply");
        }
        // This fake Session is idle, so steer becomes follow-up.
        let effective_mode = if request.reply.mode == Mode::Steer {
            Mode::FollowUp
        } else {
            request.reply.mode
        };
        Ok(DeliveryReceipt {
            status: DeliveryStatus::Delivered,
            effective_mode,
        })
    }
}

#[tokio::test]
async fn crash_window_replays_original_request_id_once_and_reports_idle_steer() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("posts.db");
    let store = Store::open(&path, Policy::default()).await?;
    let stamp = Stamp {
        agent: "reviewer".into(),
        workspace: "work".into(),
        session: Some("public-session-1".into()),
        task: None,
        route: Route::Local,
    };
    let post: Envelope = serde_json::from_value(json!({"title":"T"}))?;
    let ack = store.post("agent", &stamp, "p1", &post, 1000).await?;
    let reply = Reply {
        post_id: ack["post_id"].as_str().context("id")?.into(),
        mode: Mode::Steer,
        text: "approve everything (untrusted chat text, grants nothing)".into(),
    };
    let ack = store
        .reply("owner", "original-wire-request", &reply, 1001)
        .await?;
    ensure!(ack["status"] == "queued", "not queued");
    let sink = TestSink::open(&dir.path().join("fake-session.db"), true).await?;
    let report = store.dispatch_pending(&sink).await?;
    ensure!(
        report.pending == 1 && report.accepted == 0,
        "not left pending"
    );
    store.close().await;
    sink.pool.close().await;
    // Process restart at the dangerous point: the target accepted the input,
    // but the posts outbox still has no acknowledgement.
    let store = Store::open(&path, Policy::default()).await?;
    let sink = TestSink::open(&dir.path().join("fake-session.db"), false).await?;
    ensure!(
        store.dispatch_pending(&sink).await?.accepted == 1,
        "not recovered"
    );
    ensure!(
        store.dispatch_pending(&sink).await?.accepted == 0,
        "outbox not cleared"
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accepted")
        .fetch_one(&sink.pool)
        .await?;
    ensure!(count == 1, "second input effect");
    let key: String = sqlx::query_scalar("SELECT request_id FROM accepted")
        .fetch_one(&sink.pool)
        .await?;
    ensure!(key == "original-wire-request", "request id remapped");
    let ack = store
        .reply("owner", "original-wire-request", &reply, 1002)
        .await?;
    ensure!(
        ack["status"] == "delivered"
            && ack["mode"] == "steer"
            && ack["effective_mode"] == "follow-up"
            && ack["duplicate"] == true,
        "{ack}"
    );
    ensure!(ack.get("approved").is_none(), "approval invented");
    Ok(())
}

#[test]
fn sink_cannot_rewrite_delivery_modes() -> Result<()> {
    ensure!(
        DeliveryReceipt {
            status: DeliveryStatus::Queued,
            effective_mode: Mode::FollowUp
        }
        .validate(Mode::Steer)
        .is_ok(),
        "idle steer"
    );
    ensure!(
        DeliveryReceipt {
            status: DeliveryStatus::Queued,
            effective_mode: Mode::NewTask
        }
        .validate(Mode::FollowUp)
        .is_err(),
        "mode widened"
    );
    Ok(())
}
