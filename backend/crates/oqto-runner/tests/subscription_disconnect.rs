#![cfg(target_os = "linux")]

//! A streaming subscription whose client goes away must release its socket
//! even when the stream is silent. Regression for the runner exhausting open
//! files with sockets of departed web/mobile clients.

use anyhow::{Context, Result};
use oqto_runner::client::RunnerClient;
use std::collections::HashMap;
use std::time::Duration;

const CLIENTS: usize = 40;

fn open_fds(pid: u32) -> Result<usize> {
    Ok(std::fs::read_dir(format!("/proc/{pid}/fd"))?.count())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn silent_subscriptions_release_their_socket_when_the_client_leaves() -> Result<()> {
    let home = tempfile::tempdir()?;
    let socket = home.path().join("runner.sock");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_oqto-runner"))
        .arg("--socket")
        .arg(&socket)
        .arg("--no-sandbox")
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_STATE_HOME", home.path().join("state"))
        .env("XDG_DATA_HOME", home.path().join(".local/share"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("spawning oqto-runner")?;
    let runner = child.id();

    let result = async {
        let client = RunnerClient::new(socket.clone());
        tokio::time::timeout(Duration::from_secs(15), async {
            while client.list_sessions().await.is_err() {
                anyhow::ensure!(child.try_wait()?.is_none(), "runner exited early");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Ok(())
        })
        .await??;

        // A long-running process that never writes: its stream stays silent.
        client
            .spawn_rpc_process(
                "silent",
                "sleep",
                vec!["300".into()],
                home.path(),
                HashMap::new(),
                false,
            )
            .await?;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let baseline = open_fds(runner)?;

        let mut subscriptions = Vec::new();
        for _ in 0..CLIENTS {
            subscriptions.push(client.subscribe_stdout("silent").await?);
        }
        let held = open_fds(runner)?;
        anyhow::ensure!(
            held >= baseline + CLIENTS,
            "subscriptions not held: baseline {baseline}, with clients {held}"
        );

        drop(subscriptions);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let now = open_fds(runner)?;
            if now <= baseline + 2 {
                break;
            }
            anyhow::ensure!(
                tokio::time::Instant::now() < deadline,
                "leaked sockets: baseline {baseline}, after clients left {now}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    result
}
