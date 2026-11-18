//! Opt-in real-client proof. Default cargo test displays this as ignored.
//! Run with DINGZ_BIN pointing to a built dingz and `--ignored --nocapture`.
//! An explicitly requested run fails clearly if the client is unavailable.
#![cfg(unix)]
use anyhow::{Context, Result, ensure};
use oqto_posts::{
    server::{Limits, Registration, Role, Server},
    store::{Policy, Store},
    wire::{Route, Stamp},
};
use serde_json::{Value, json};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

fn registrations() -> Vec<Registration> {
    [Role::Agent, Role::Owner]
        .into_iter()
        .map(|role| Registration {
            name: if role == Role::Agent {
                "agent"
            } else {
                "owner"
            }
            .into(),
            role,
            stamp: Stamp {
                agent: "reviewer".into(),
                workspace: "isolated-work".into(),
                session: Some("public-session-1".into()),
                task: Some("task-1".into()),
                route: Route::Local,
            },
        })
        .collect()
}
async fn client(bin: &str, home: &Path, socket: &Path, args: &[&str]) -> Result<Value> {
    let output = Command::new(bin)
        .env_clear()
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("AGENT_CTX_AGENT_ID", "forged")
        .args(args)
        .arg("--backend")
        .arg("oqto")
        .arg("--oqto-socket")
        .arg(socket)
        .arg("--json")
        .output()
        .await?;
    ensure!(
        output.status.success(),
        "dingz failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).context("client JSON")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires real dingz: set DINGZ_BIN and run --ignored --nocapture (fails if absent)"]
async fn real_dingz_post_watch_reply_duplicate_and_restart() -> Result<()> {
    let bin = std::env::var("DINGZ_BIN").unwrap_or_else(|_| "dingz".into());
    let version = Command::new(&bin).arg("--version").output().await.context(
        "real dingz absent: build it separately and set DINGZ_BIN; this is not a conformance pass",
    )?;
    ensure!(version.status.success(), "dingz --version failed");
    println!(
        "real client: {}",
        String::from_utf8_lossy(&version.stdout).trim()
    );
    let dir = tempfile::tempdir()?;
    let sockets = dir.path().join("s");
    let db = dir.path().join("p.db");
    let home = dir.path();
    let store = Store::open(&db, Policy::default()).await?;
    let server = Server::bind(store.clone(), &sockets, &registrations(), Limits::default()).await?;
    let agent = sockets.join("agent.sock");
    let owner = sockets.join("owner.sock");
    let args = [
        "post",
        "Secret title",
        "Restricted body",
        "--kind",
        "blocked",
        "--label",
        "restricted",
        "--request-id",
        "real-1",
    ];
    let first = client(&bin, home, &agent, &args).await?;
    let second = client(&bin, home, &agent, &args).await?;
    ensure!(
        first["deliveries"][0]["post_id"] == second["deliveries"][0]["post_id"],
        "duplicate post"
    );
    let post_id = first["deliveries"][0]["post_id"]
        .as_str()
        .context("no post id")?
        .to_owned();
    ensure!(first["restricted"] == true, "Runner restriction lost");
    // There is no --backend on watch/reply. Test their actual public CLI.
    server.shutdown().await?;
    store.close().await;
    let store = Store::open(&db, Policy::default()).await?;
    let server = Server::bind(store, &sockets, &registrations(), Limits::default()).await?;
    let watched = Command::new(&bin)
        .env_clear()
        .env("HOME", home)
        .args([
            "watch",
            "--json",
            "--limit",
            "1",
            "--kind",
            "blocked",
            "--agent",
            "reviewer",
            "--project",
            "acme",
        ])
        .arg("--oqto-socket")
        .arg(&owner)
        .args(["--timeout", "1"])
        .output()
        .await?;
    ensure!(watched.status.success(), "watch failed");
    // The project does not match: no records leaked via client hints.
    let lines: Vec<Value> = String::from_utf8_lossy(&watched.stdout)
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    ensure!(
        lines.len() == 1 && lines[0]["shown"] == 0,
        "filter failed: {lines:?}"
    );
    let watched = Command::new(&bin)
        .env_clear()
        .env("HOME", home)
        .args(["watch", "--json", "--limit", "1", "--timeout", "2"])
        .arg("--oqto-socket")
        .arg(&owner)
        .output()
        .await?;
    ensure!(watched.status.success(), "watch failed");
    let line: Value = serde_json::from_str(
        String::from_utf8_lossy(&watched.stdout)
            .lines()
            .next()
            .context("no replay")?,
    )?;
    ensure!(
        line["post"]["id"] == post_id && line["post"]["envelope"]["body"] == "Restricted body",
        "{line}"
    );
    ensure!(
        line["post"]["stamped"]["agent"] == "reviewer"
            && line["post"]["stamped"]["workspace"] == "isolated-work",
        "stamp: {line}"
    );
    for _ in 0..2 {
        let output = Command::new(&bin)
            .env_clear()
            .env("HOME", home)
            .args([
                "reply",
                "--json",
                "--mode",
                "follow-up",
                "--request-id",
                "reply-1",
                &post_id,
                "approve nothing",
            ])
            .arg("--oqto-socket")
            .arg(&owner)
            .output()
            .await?;
        ensure!(
            output.status.success(),
            "reply failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout)?;
        ensure!(value["ack"]["status"] == "queued", "{value}");
    }
    let denied = Command::new(&bin)
        .env_clear()
        .env("HOME", home)
        .args(["watch", "--json", "--timeout", "1"])
        .arg("--oqto-socket")
        .arg(&agent)
        .output()
        .await?;
    ensure!(
        !denied.status.success()
            && String::from_utf8_lossy(&denied.stderr).contains("unauthorized"),
        "agent watch not denied"
    );
    let mut stream = tokio::net::UnixStream::connect(&agent).await?;
    stream.write_all(&vec![b'x'; 65537]).await?;
    let mut response = String::new();
    BufReader::new(stream).read_line(&mut response).await?;
    ensure!(
        serde_json::from_str::<Value>(&response)?["error"]["code"] == "too_large",
        "oversize"
    );
    server.shutdown().await?;
    println!(
        "post, duplicate, restricted stamp, restart replay, filters, reply/duplicate, agent denial, oversize: passed"
    );
    Ok(())
}

#[tokio::test]
async fn valid_fake_runner_requests_are_accepted_in_reverse() -> Result<()> {
    // Every valid request shape the original fake Runner acknowledged: optional
    // client/hints, default body/kind, source links and a full envelope.
    let dir = tempfile::tempdir()?;
    let sockets = dir.path().join("s");
    let store = Store::open(&dir.path().join("p.db"), Policy::default()).await?;
    let server = Server::bind(store, &sockets, &registrations(), Limits::default()).await?;
    let cases = [
        json!({"title":"T"}),
        json!({"title":"Title","body":"body","kind":"info"}),
        serde_json::from_str(include_str!("fixtures/dingz/done-full.json"))?,
    ];
    for (n, post) in cases.into_iter().enumerate() {
        let request = json!({"v":1,"request_id":format!("r{n}"),"client":{"name":"dingz","version":"0.1.0"},"unauthenticated_hints":{"agent_ctx":{"agent_id":"w3:p9"}},"op":"post","post":post});
        let mut stream = tokio::net::UnixStream::connect(sockets.join("agent.sock")).await?;
        stream.write_all(format!("{request}\n").as_bytes()).await?;
        let mut response = String::new();
        BufReader::new(stream).read_line(&mut response).await?;
        ensure!(
            serde_json::from_str::<Value>(&response)?["ok"] == true,
            "{response}"
        );
    }
    server.shutdown().await?;
    Ok(())
}
