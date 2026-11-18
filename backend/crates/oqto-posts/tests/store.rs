use anyhow::{Result, ensure};
use oqto_posts::{
    store::{Policy, Store, sequence},
    wire::{Envelope, Filter, Mode, Reply, Route, Stamp},
};
use serde_json::json;

fn stamp() -> Stamp {
    Stamp {
        agent: "reviewer".into(),
        workspace: "work".into(),
        session: Some("public-session-1".into()),
        task: Some("task-1".into()),
        route: Route::Local,
    }
}
fn envelope() -> Result<Envelope> {
    Ok(serde_json::from_value(
        json!({"title":"T","body":"secret","task":"forged","labels":["route:cloud","untrusted-source"]}),
    )?)
}

#[tokio::test]
async fn durable_receipts_posts_replies_and_conflicts() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("posts.db");
    let store = Store::open(&path, Policy::default()).await?;
    let ack = store
        .post("socket-a", &stamp(), "r1", &envelope()?, 1000)
        .await?;
    let post_id = ack["post_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("no id"))?;
    let reply = Reply {
        post_id: post_id.into(),
        mode: Mode::FollowUp,
        text: "approve nothing".into(),
    };
    let reply_ack = store.reply("owner", "r2", &reply, 1001).await?;
    store.close().await;
    let store = Store::open(&path, Policy::default()).await?;
    let duplicate = store
        .post("socket-a", &stamp(), "r1", &envelope()?, 1002)
        .await?;
    ensure!(
        duplicate["post_id"] == ack["post_id"] && duplicate["duplicate"] == true,
        "{duplicate}"
    );
    let repeat = store.reply("owner", "r2", &reply, 1003).await?;
    ensure!(
        repeat["reply_id"] == reply_ack["reply_id"] && repeat["duplicate"] == true,
        "{repeat}"
    );
    let mut changed = envelope()?;
    changed.body = "changed".into();
    ensure!(
        store
            .post("socket-a", &stamp(), "r1", &changed, 1004)
            .await
            .is_err(),
        "conflict accepted"
    );
    let changed_reply = Reply {
        text: "changed".into(),
        ..reply
    };
    ensure!(
        store
            .reply("owner", "r2", &changed_reply, 1004)
            .await
            .is_err(),
        "reply conflict"
    );
    let page = store.page(0, &Filter::default(), 1005).await?;
    ensure!(page.posts.len() == 1, "second effect");
    let p = &page.posts[0].1;
    ensure!(
        p.stamped.agent == "reviewer" && p.stamped.workspace == "work",
        "stamp"
    );
    ensure!(
        p.envelope.task.as_deref() == Some("task-1"),
        "task claim retained"
    );
    ensure!(
        p.envelope.labels == ["untrusted-source", "route:local"],
        "route widened"
    );
    ensure!(page.through == 1, "cursor");
    ensure!(
        store
            .page(1, &Filter::default(), 1005)
            .await?
            .posts
            .is_empty(),
        "replayed boundary twice"
    );
    Ok(())
}

#[tokio::test]
async fn retention_is_count_and_age_bounded_but_dedupe_survives() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let policy = Policy {
        max_posts: 2,
        max_age_secs: 10,
        posts_per_minute: 100,
    };
    let store = Store::open(&dir.path().join("p.db"), policy).await?;
    let first = store.post("a", &stamp(), "r1", &envelope()?, 100).await?;
    for n in 2..=4 {
        store
            .post("a", &stamp(), &format!("r{n}"), &envelope()?, 100 + n)
            .await?;
    }
    let page = store.page(0, &Filter::default(), 105).await?;
    ensure!(
        page.posts.len() == 2 && page.gap == Some(2) && page.through == 4,
        "{page:?}"
    );
    let old = store.post("a", &stamp(), "r1", &envelope()?, 110).await?;
    ensure!(
        old["post_id"] == first["post_id"] && old["duplicate"] == true,
        "old key reused"
    );
    let page = store.page(2, &Filter::default(), 120).await?;
    ensure!(page.posts.is_empty() && page.gap == Some(4), "{page:?}");
    Ok(())
}

#[tokio::test]
async fn rate_limit_and_concurrent_duplicates() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let store = Store::open(
        &dir.path().join("p.db"),
        Policy {
            posts_per_minute: 1,
            ..Policy::default()
        },
    )
    .await?;
    let post = envelope()?;
    let stamp = stamp();
    let (a, b) = tokio::join!(
        store.post("a", &stamp, "r1", &post, 1000),
        store.post("a", &stamp, "r1", &post, 1000)
    );
    ensure!(
        a?["post_id"] == b?["post_id"],
        "duplicate concurrent effects"
    );
    let error = store
        .post("a", &stamp, "r2", &post, 1001)
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("rate not limited"))?;
    ensure!(error.to_string().contains("rate_limited"), "{error}");
    store.post("other", &stamp, "r2", &post, 1001).await?;
    store.post("a", &stamp, "r2", &post, 1061).await?;
    Ok(())
}

#[tokio::test]
async fn filter_and_stricter_routes() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let store = Store::open(&dir.path().join("p.db"), Policy::default()).await?;
    let mut post = envelope()?;
    post.project = Some("acme/tool".into());
    post.funnel = Some("intake".into());
    let mut stamp = stamp();
    stamp.route = Route::Cloud;
    post.labels = vec!["route:private".into()];
    store.post("a", &stamp, "r1", &post, 1000).await?;
    let filter = Filter {
        agents: vec!["reviewer".into()],
        projects: vec!["acme".into()],
        funnels: vec!["intake".into()],
        labels: vec!["route:private".into()],
        ..Filter::default()
    };
    ensure!(
        store.page(0, &filter, 1001).await?.posts.len() == 1,
        "filter"
    );
    let filter = Filter {
        agents: vec!["forged".into()],
        ..filter
    };
    let page = store.page(0, &filter, 1001).await?;
    ensure!(
        page.posts.is_empty() && page.through == 1,
        "filtered cursor failed"
    );
    for valid in ["c-0", "c-123"] {
        sequence(valid)?;
    }
    for invalid in ["-1", "c--1", "c-9223372036854775808", "x-0"] {
        ensure!(sequence(invalid).is_err(), "bad cursor");
    }
    Ok(())
}
