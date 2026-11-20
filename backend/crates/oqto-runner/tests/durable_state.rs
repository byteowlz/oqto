//! Real kernel lock lifetime in a disposable second test process, never a brain.
#![cfg(unix)]
use anyhow::{Context, Result, ensure};
use oqto_runner::{durable_catalog::AgentHome, durable_state::DurableState};
use std::{os::unix::fs::MetadataExt, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};
fn home(agent: &str) -> AgentHome {
    AgentHome {
        workspace_id: "process-fixture".into(),
        agent_id: agent.into(),
    }
}

#[tokio::test]
#[ignore = "helper process executed explicitly by process_death_releases_store_ownership_without_unlinking"]
async fn child_lease_fixture() -> Result<()> {
    let root =
        std::env::var_os("OQTO_DURABLE_TEST_ROOT").context("parent fixture root required")?;
    let state = DurableState::open(std::path::Path::new(&root)).await?;
    let lease = state.lease_store(&home("agent")).await?;
    eprintln!("LEASE_READY {}", lease.record().store_id);
    let mut byte = [0];
    std::io::Read::read_exact(&mut std::io::stdin(), &mut byte)?;
    state.close().await;
    Ok(())
}

#[tokio::test]
async fn process_death_releases_store_ownership_without_unlinking() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("state");
    let state = DurableState::open(&root).await?;
    let mut child = Command::new(std::env::current_exe()?)
        .args([
            "--ignored",
            "--exact",
            "child_lease_fixture",
            "--nocapture",
            "--test-threads=1",
        ])
        .env_clear()
        .env("OQTO_DURABLE_TEST_ROOT", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    // Libtest writes status prefixes on stdout in single-thread mode. Keep the
    // fixture's explicit readiness channel separate rather than parsing those logs.
    let mut lines =
        BufReader::new(child.stderr.take().context("fixture readiness channel")?).lines();
    let id = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(line) = lines.next_line().await? {
            if let Some(id) = line.strip_prefix("LEASE_READY ") {
                return Ok::<_, anyhow::Error>(id.to_owned());
            }
        }
        anyhow::bail!("child exited without store ownership")
    })
    .await??;
    ensure!(
        uuid::Uuid::parse_str(&id)?.to_string() == id,
        "invalid fixture store id"
    );
    let lock_path = root.join("control/locks").join(format!("{id}.lock"));
    let inode = std::fs::metadata(&lock_path)?.ino();
    assert!(state.lease_store(&home("agent")).await.is_err());
    let other = state.lease_store(&home("other")).await?;
    other.revalidate().await?;
    child.kill().await?;
    tokio::time::timeout(Duration::from_secs(5), child.wait()).await??;
    let new = state.lease_store(&home("agent")).await?;
    assert_eq!(new.record().store_id, id);
    assert_eq!(std::fs::metadata(&lock_path)?.ino(), inode);
    new.revalidate().await?;
    state.close().await;
    Ok(())
}
