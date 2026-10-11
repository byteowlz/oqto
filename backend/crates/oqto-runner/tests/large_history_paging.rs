#![cfg(target_os = "linux")]

//! A session whose history exceeds the runner frame limit must still load:
//! a single unbounded reply is dropped by the runner, so complete-history
//! readers page. Regression for chats rendering "No messages yet".

use anyhow::{Context, Result};
use oqto_history::oqto_log::store::{PiJsonlMessageRecord, replace_session_with_pi_jsonl_records};
use oqto_pi::AgentMessage;
use oqto_runner::client::RunnerClient;
use oqto_runner::protocol::WorkspaceChatMessagesSource;
use oqto_runner::wire::MAX_RUNNER_FRAME_BYTES;
use std::time::Duration;

const SESSION: &str = "oqto-large-history";
const MESSAGES: usize = 24;

fn message(index: usize, body: &str) -> AgentMessage {
    AgentMessage {
        role: if index % 2 == 0 { "user" } else { "assistant" }.to_string(),
        content: serde_json::Value::String(format!("#{index:03} {body}")),
        timestamp: Some(1_000 + index as u64),
        tool_call_id: None,
        tool_name: None,
        is_error: None,
        api: None,
        provider: None,
        model: None,
        usage: None,
        stop_reason: None,
        extra: Default::default(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_larger_than_one_frame_loads_completely_and_in_order() -> Result<()> {
    let home = tempfile::tempdir()?;
    // ~24 x 1 MiB of text: well over the 16 MiB frame limit in total.
    let body = "x".repeat(1024 * 1024);
    let records: Vec<PiJsonlMessageRecord> = (0..MESSAGES)
        .map(|i| PiJsonlMessageRecord {
            source_entry_id: format!("entry-{i}"),
            parent_source_entry_id: i.checked_sub(1).map(|p| format!("entry-{p}")),
            source_sequence: i as i64,
            message: message(i, &body),
        })
        .collect();
    replace_session_with_pi_jsonl_records(
        home.path(),
        "user-1",
        "/tmp/oqto-large-history",
        SESSION,
        SESSION,
        Some("external-large"),
        "external-large",
        &records,
    )
    .await
    .context("seeding oqto-log")?;
    assert!(MESSAGES * body.len() > MAX_RUNNER_FRAME_BYTES);

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

        // The bug: one unbounded reply cannot cross the frame limit.
        let unbounded = client
            .get_workspace_chat_session_messages(
                SESSION,
                false,
                None,
                WorkspaceChatMessagesSource::Authoritative,
            )
            .await;
        anyhow::ensure!(unbounded.is_err(), "unbounded reply unexpectedly fit");

        // The newest page is bounded and carries a cursor to continue.
        let first = client
            .get_workspace_chat_session_messages_paged(
                SESSION,
                false,
                Some(5),
                None,
                WorkspaceChatMessagesSource::Authoritative,
            )
            .await?;
        anyhow::ensure!(first.messages.len() == 5, "{}", first.messages.len());
        anyhow::ensure!(first.has_more && first.next_before.is_some());

        // Complete history: every message exactly once, oldest first.
        let all = client
            .get_workspace_chat_session_messages_all(SESSION, false)
            .await?;
        let stamps: Vec<i64> = all.messages.iter().map(|m| m.created_at).collect();
        let expected: Vec<i64> = (0..MESSAGES as i64).map(|i| (1_000 + i) * 1_000).collect();
        anyhow::ensure!(stamps == expected, "order/completeness: {stamps:?}");
        Ok(())
    }
    .await;

    let _ = child.kill();
    let _ = child.wait();
    result
}
