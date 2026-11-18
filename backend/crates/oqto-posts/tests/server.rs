#![cfg(unix)]
mod common;
use anyhow::{Result, ensure};
use common::*;
use oqto_posts::server::{Limits, check_peer};
use serde_json::{Value, json};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use tokio::{
    io::{AsyncWriteExt, BufReader},
    net::UnixStream,
};

#[tokio::test]
async fn socket_identity_not_claims_and_shutdown_end() -> Result<()> {
    let h = Harness::new(Limits::default()).await?;
    ensure!(
        std::fs::metadata(&h.sockets)?.mode() & 0o777 == 0o700,
        "directory"
    );
    ensure!(
        std::fs::metadata(h.path("agent"))?.mode() & 0o777 == 0o600,
        "socket"
    );
    let ack = request(&h.path("agent"), post("r1")).await?;
    ensure!(
        ack["ok"] == true && ack["result"]["labels"] == json!(["route:local"]),
        "{ack}"
    );
    for op in ["watch", "reply"] {
        let ack=request(&h.path("agent"),json!({"v":1,"request_id":"x","op":op,"reply":{"post_id":"p","mode":"steer","text":"approve"}})).await?;
        ensure!(ack["error"]["code"] == "unauthorized", "{ack}");
    }
    let ack = request(&h.path("owner"), post("r1")).await?;
    ensure!(
        ack["error"]["code"] == "unauthorized",
        "owner may not impersonate agent"
    );
    let mut stream = UnixStream::connect(h.path("owner")).await?;
    stream
        .write_all(b"{\"v\":1,\"request_id\":\"w\",\"op\":\"watch\"}\n")
        .await?;
    let mut reader = BufReader::new(stream);
    ensure!(line(&mut reader).await?["ok"] == true, "watch ack");
    let event = line(&mut reader).await?;
    ensure!(
        event["post"]["stamped"]["agent"] == "agent-agent",
        "{event}"
    );
    ensure!(event["post"]["stamped"]["workspace"] == "work", "{event}");
    ensure!(event["post"]["envelope"]["task"] == "task-1", "{event}");
    let path = h.path("owner");
    h.server.shutdown().await?;
    ensure!(line(&mut reader).await?["event"] == "end", "shutdown end");
    ensure!(!path.exists(), "socket left behind");
    Ok(())
}

#[tokio::test]
async fn frames_versions_timeouts_limits_and_no_approve() -> Result<()> {
    let h = Harness::new(Limits {
        max_connections: 1,
        request_timeout_ms: 300,
        write_timeout_ms: 300,
    })
    .await?;
    // A connection holding a partial request consumes the only permit.
    let mut held = UnixStream::connect(h.path("agent")).await?;
    held.write_all(b"{").await?;
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    let ack = request(&h.path("agent"), post("r1")).await?;
    ensure!(ack["error"]["code"] == "unavailable", "{ack}");
    ensure!(
        line(&mut BufReader::new(held)).await?["error"]["code"] == "invalid_request",
        "timeout"
    );
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    for (value, code) in [
        (
            json!({"v":2,"op":"post","request_id":"r"}),
            "unsupported_version",
        ),
        (
            json!({"v":1,"op":"approve","request_id":"r"}),
            "invalid_request",
        ),
        (
            json!({"v":1,"op":"post","request_id":"r","post":{"title":"T","kind":"evil"}}),
            "invalid_request",
        ),
    ] {
        let ack = request(&h.path("agent"), value).await?;
        ensure!(ack["error"]["code"] == code, "{ack}");
    }
    let mut huge = UnixStream::connect(h.path("agent")).await?;
    huge.write_all(&vec![b'x'; 65537]).await?;
    let ack = line(&mut BufReader::new(huge)).await?;
    ensure!(ack["error"]["code"] == "too_large", "{ack}");
    h.server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn owner_watch_sees_workspaces_but_agent_cannot_read_other_workspace() -> Result<()> {
    use oqto_posts::{
        server::{Role, Server},
        store::{Policy, Store},
    };
    let dir = tempfile::tempdir()?;
    let sockets = dir.path().join("s");
    let store = Store::open(&dir.path().join("p.db"), Policy::default()).await?;
    let mut a = registration("a", Role::Agent);
    a.stamp.workspace = "isolated-a".into();
    let mut b = registration("b", Role::Agent);
    b.stamp.workspace = "isolated-b".into();
    let server = Server::bind(
        store,
        &sockets,
        &[a, b, registration("owner", Role::Owner)],
        Limits::default(),
    )
    .await?;
    for name in ["a", "b"] {
        request(&sockets.join(format!("{name}.sock")), post("r1")).await?;
    }
    for name in ["a", "b"] {
        let ack = request(
            &sockets.join(format!("{name}.sock")),
            json!({"v":1,"request_id":"w","op":"watch"}),
        )
        .await?;
        ensure!(ack["error"]["code"] == "unauthorized", "{ack}");
    }
    let mut stream = UnixStream::connect(sockets.join("owner.sock")).await?;
    stream.write_all(b"{\"v\":1,\"request_id\":\"w\",\"op\":\"watch\",\"filter\":{\"agents\":[\"b-agent\"]}}\n").await?;
    let mut reader = BufReader::new(stream);
    line(&mut reader).await?;
    let value = line(&mut reader).await?;
    ensure!(
        value["post"]["stamped"]["workspace"] == "isolated-b",
        "{value}"
    );
    server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn storage_failure_closes_without_claiming_a_definite_rejection() -> Result<()> {
    use tokio::io::AsyncBufReadExt;
    let h = Harness::new(Limits::default()).await?;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(h.dir.path().join("p.db")))
        .await?;
    sqlx::query("CREATE TRIGGER fail_receipt BEFORE INSERT ON receipts BEGIN SELECT RAISE(ABORT, 'synthetic storage failure'); END").execute(&pool).await?;
    let mut stream = UnixStream::connect(h.path("agent")).await?;
    stream
        .write_all(format!("{}\n", post("r1")).as_bytes())
        .await?;
    let mut response = String::new();
    let n = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        BufReader::new(stream).read_line(&mut response),
    )
    .await??;
    ensure!(
        n == 0,
        "uncertain storage error claimed a rejection: {response}"
    );
    sqlx::query("DROP TRIGGER fail_receipt")
        .execute(&pool)
        .await?;
    let ack = request(&h.path("agent"), post("r1")).await?;
    ensure!(
        ack["ok"] == true && ack["result"].get("duplicate").is_none(),
        "failed admission left an effect: {ack}"
    );
    h.server.shutdown().await?;
    Ok(())
}

#[test]
fn peer_check_fails_closed_and_directory_permissions_not_repaired() -> Result<()> {
    use oqto_posts::server::private_directory;
    ensure!(
        check_peer(501, 501).is_ok()
            && check_peer(0, 501).is_err()
            && check_peer(502, 501).is_err(),
        "peer check"
    );
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("s");
    private_directory(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    ensure!(
        private_directory(&path).is_err(),
        "insecure directory accepted"
    );
    ensure!(
        std::fs::metadata(&path)?.mode() & 0o777 == 0o755,
        "mutated caller directory"
    );
    Ok(())
}

#[tokio::test]
async fn disconnect_releases_watch_permit() -> Result<()> {
    let h = Harness::new(Limits {
        max_connections: 1,
        ..Limits::default()
    })
    .await?;
    let mut stream = UnixStream::connect(h.path("owner")).await?;
    stream
        .write_all(b"{\"v\":1,\"request_id\":\"w\",\"op\":\"watch\"}\n")
        .await?;
    let mut reader = BufReader::new(stream);
    line(&mut reader).await?;
    drop(reader);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let ack = request(&h.path("agent"), post("r1")).await?;
    ensure!(
        ack["ok"] == true,
        "disconnected watcher leaked permit: {ack}"
    );
    h.server.shutdown().await?;
    // Keep the fixture's directory alive to the end of the server lifetime.
    ensure!(h.dir.path().exists(), "temp directory removed early");
    Ok(())
}

#[tokio::test]
async fn stale_cursor_reports_gap_then_replays_retained_posts() -> Result<()> {
    use oqto_posts::{
        server::{Role, Server},
        store::{Policy, Store},
    };
    let dir = tempfile::tempdir()?;
    let sockets = dir.path().join("s");
    let store = Store::open(
        &dir.path().join("p.db"),
        Policy {
            max_posts: 1,
            ..Policy::default()
        },
    )
    .await?;
    let server = Server::bind(
        store,
        &sockets,
        &[
            registration("agent", Role::Agent),
            registration("owner", Role::Owner),
        ],
        Limits::default(),
    )
    .await?;
    for id in ["r1", "r2"] {
        request(&sockets.join("agent.sock"), post(id)).await?;
    }
    let mut stream = UnixStream::connect(sockets.join("owner.sock")).await?;
    stream
        .write_all(b"{\"v\":1,\"request_id\":\"w\",\"op\":\"watch\",\"since\":\"c-0\"}\n")
        .await?;
    let mut reader = BufReader::new(stream);
    line(&mut reader).await?;
    let gap: Value = line(&mut reader).await?;
    ensure!(
        gap["event"] == "gap" && gap["retained_after"] == "c-1",
        "{gap}"
    );
    ensure!(line(&mut reader).await?["cursor"] == "c-2", "replay");
    server.shutdown().await?;
    Ok(())
}
