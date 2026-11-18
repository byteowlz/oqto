//! Against a real byoagnt chat host (Pi Durable 1.5.2, scripted faux model) on loopback.
//!
//! Needs `OQT_BYOAGNT_DIR` pointing at a byoagnt checkout with dependencies installed and
//! `bun` on PATH; without it the test says so and does nothing. The secret is random per run
//! and reaches the host only through its environment.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use oqto_session_client::{
    FileCache, KeySource, Link, Options, Secret, SessionConfig, SessionHandle,
};
use oqto_session_view::frames::InputKind;
use serde_json::Value;

struct Host {
    child: Child,
    url: String,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn secret() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn getrandom(bytes: &mut [u8]) -> Result<()> {
    let file = std::fs::File::open("/dev/urandom").context("urandom")?;
    std::io::Read::read_exact(&mut &file, bytes).context("read urandom")
}

fn start_host(dir: &str, storage: &std::path::Path, secret: &str) -> Result<Host> {
    let mut child = Command::new("bun")
        .arg("examples/chat/main.ts")
        .current_dir(dir)
        .env("BYOAGNT_SECRET", secret)
        .env("BYOAGNT_STORAGE", storage)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("start bun")?;
    let stdout = child.stdout.take().context("host stdout")?;
    let mut line = String::new();
    BufReader::new(stdout)
        .read_line(&mut line)
        .context("read host URL")?;
    let page = line.trim().trim_end_matches('/');
    if !page.starts_with("http://127.0.0.1:") {
        bail!("unexpected host output");
    }
    Ok(Host {
        child,
        url: format!("{}/session", page.replacen("http", "ws", 1)),
    })
}

fn config(url: &str, secret: &str, cache: Arc<FileCache>) -> Result<SessionConfig> {
    Ok(SessionConfig {
        url: url.to_owned(),
        secret: Secret::new(secret.to_owned())?,
        session: "ses_chat".to_owned(),
        options: Options::default(),
        cache: Some(cache),
        max_cached_entries: 2000,
        connect_timeout: Duration::from_secs(5),
    })
}

fn text_of(entry: &Value) -> String {
    let content = entry.pointer("/model/0/content");
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect(),
        _ => String::new(),
    }
}

async fn wait_for(
    handle: &SessionHandle,
    what: &str,
    check: impl Fn(&SessionHandle) -> bool,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut changes = handle.changed();
    while !check(handle) {
        if Instant::now() > deadline {
            bail!("timed out waiting for {what}");
        }
        let _ = tokio::time::timeout(Duration::from_millis(200), changes.changed()).await;
    }
    Ok(())
}

#[tokio::test]
async fn chat_round_trip_cache_first_restart_and_tool_output() -> Result<()> {
    let Ok(dir) = std::env::var("OQT_BYOAGNT_DIR") else {
        println!("OQT_BYOAGNT_DIR not set: live byoagnt host test not run");
        return Ok(());
    };
    let temp = tempfile::tempdir()?;
    let secret = secret()?;
    let host = start_host(&dir, &temp.path().join("store"), &secret)?;
    let cache = Arc::new(FileCache::open(
        temp.path().join("cache"),
        &KeySource::File(temp.path().join("key")),
    )?);

    let (handle, task) = SessionHandle::start(config(&host.url, &secret, cache.clone())?);
    wait_for(&handle, "online", |h| {
        h.read(|s| s.link == Link::Online && s.client.view().ready())
    })
    .await?;
    let request = handle.update(|c| c.submit(InputKind::Prompt, "echo hello from rust"));
    wait_for(&handle, "answer", |h| {
        h.read(|s| {
            s.client
                .receipt(&request)
                .is_some_and(|r| r.status.is_final())
                && !s.client.view().working()
        })
    })
    .await?;
    let (texts, kinds) = handle.read(|s| {
        let entries = s.client.transcript().entries();
        (
            entries
                .iter()
                .map(|e| text_of(e.value()))
                .collect::<Vec<_>>(),
            entries
                .iter()
                .map(|e| e.kind().to_owned())
                .collect::<Vec<_>>(),
        )
    });
    assert!(
        texts.iter().any(|t| t == "echo hello from rust"),
        "user entry: {texts:?}"
    );
    assert!(
        kinds.iter().any(|k| k == "pi.tool-result"),
        "tool result: {kinds:?}"
    );
    assert!(
        texts
            .iter()
            .any(|t| t.contains("The echo tool returned: hello from rust")),
        "answer: {texts:?}"
    );
    task.abort();

    // Fast start: a new client shows the cached transcript before any frame arrives.
    let started = Instant::now();
    let (second, task) = SessionHandle::start(config(&host.url, &secret, cache)?);
    let cached = second.read(|s| (s.client.cached(), s.client.transcript().entries().len()));
    println!(
        "cache-first start: {} entries shown after {:?}",
        cached.1,
        started.elapsed()
    );
    assert!(cached.0 && cached.1 >= 4, "cached start: {cached:?}");
    wait_for(&second, "live again", |h| {
        h.read(|s| s.link == Link::Online && s.client.view().ready() && !s.client.cached())
    })
    .await?;
    let gaps = second.read(|s| s.client.transcript().gaps().len());
    assert_eq!(
        gaps, 0,
        "resume after the newest cached entry leaves no gap"
    );
    task.abort();
    drop(host);
    Ok(())
}

#[tokio::test]
async fn wrong_secret_is_refused_without_echo() -> Result<()> {
    let Ok(dir) = std::env::var("OQT_BYOAGNT_DIR") else {
        println!("OQT_BYOAGNT_DIR not set: live byoagnt host test not run");
        return Ok(());
    };
    let temp = tempfile::tempdir()?;
    let host = start_host(&dir, &temp.path().join("store"), &secret()?)?;
    let wrong = secret()?;
    let error = oqto_session_client::transport::connect(
        &host.url,
        &Secret::new(wrong.clone())?,
        Duration::from_secs(5),
    )
    .await;
    let Err(error) = error else {
        bail!("a wrong secret was accepted")
    };
    let text = format!("{error:#}");
    assert!(text.contains("401"), "{text}");
    assert!(!text.contains(&wrong));
    Ok(())
}
