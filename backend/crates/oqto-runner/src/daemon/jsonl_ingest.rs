//! Serialized background ingestion of Pi JSONL session files into oqto-log.
//!
//! Pi owns the JSONL session files and can be driven outside Oqto (bare `pi`
//! continuing a session), so oqto-log needs a way to notice out-of-band
//! changes. That detection must never happen on the read path: chat reads are
//! read-only and serve oqto-log directly.
//!
//! This module is the single writer that reconciles JSONL into oqto-log:
//! - A per-file ingest cursor (size + mtime, stored next to the session
//!   index) turns "did anything change?" into one `stat()` call.
//! - A periodic sweep walks all session files as a safety net; chat opens
//!   nudge a specific session for prompt pickup.
//! - Files modified within the quiescence window are skipped: live sessions
//!   are fed to oqto-log by the event pipeline, and half-written files
//!   should not be parsed.
//! - All ingestion runs on one task, so concurrent repairs can no longer
//!   contend on workspace database locks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use log::{debug, warn};
use oqto_history::oqto_log::index::{self, IngestCursor};
use oqto_history::oqto_log::{ops, store};
use tokio::sync::mpsc;

const SWEEP_INTERVAL: Duration = Duration::from_secs(300);
const QUIESCENCE: Duration = Duration::from_secs(30);
const NUDGE_DEDUPE_WINDOW: Duration = Duration::from_secs(10);

/// Handle for nudging the ingest task about a session that was just read.
#[derive(Clone)]
pub struct IngestHandle {
    tx: mpsc::UnboundedSender<String>,
}

impl IngestHandle {
    /// Ask the ingest task to check one session's JSONL file. Fire-and-forget.
    pub fn nudge(&self, session_or_external_id: &str) {
        let _ = self.tx.send(session_or_external_id.to_string());
    }
}

/// Spawn the background ingest task. Outside a tokio runtime (unit tests
/// constructing a `Runner` directly) the task is not spawned and nudges are
/// no-ops.
pub fn spawn() -> IngestHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(run(rx));
    }
    IngestHandle { tx }
}

async fn run(mut rx: mpsc::UnboundedReceiver<String>) {
    let Ok(home) = std::env::var("HOME") else {
        return;
    };
    let home = PathBuf::from(home);
    let user_id = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());

    let mut tick = tokio::time::interval(SWEEP_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut recent: HashMap<String, Instant> = HashMap::new();

    loop {
        tokio::select! {
            _ = tick.tick() => {
                sweep(&home, &user_id).await;
            }
            nudged = rx.recv() => {
                let Some(id) = nudged else {
                    return;
                };
                let mut ids = vec![id];
                while let Ok(more) = rx.try_recv() {
                    ids.push(more);
                }
                ids.sort();
                ids.dedup();
                let now = Instant::now();
                recent.retain(|_, seen| now.duration_since(*seen) < NUDGE_DEDUPE_WINDOW);
                for id in ids {
                    if recent.contains_key(&id) {
                        continue;
                    }
                    recent.insert(id.clone(), now);
                    handle_nudge(&home, &user_id, &id).await;
                }
            }
        }
    }
}

async fn handle_nudge(home: &Path, user_id: &str, id: &str) {
    let external_id = if id.starts_with("oqto-") {
        match ops::find_external_by_session(home, id).await {
            Some(external_id) => external_id,
            None => return,
        }
    } else {
        id.to_string()
    };

    let Some(path) =
        oqto_pi::session_files::find_session_file_async(external_id.clone(), None).await
    else {
        return;
    };
    let workspace_hint = path
        .parent()
        .and_then(|dir| dir.file_name())
        .and_then(|name| name.to_str())
        .and_then(super::server::decode_workspace_path_from_safe_dirname);
    maybe_ingest(
        home,
        user_id,
        &external_id,
        &path,
        workspace_hint.as_deref(),
    )
    .await;
}

async fn sweep(home: &Path, user_id: &str) {
    let started = Instant::now();
    let mut checked = 0usize;
    for (path, workspace_hint) in list_session_files_with_workspace(home) {
        let Some(external_id) = super::server::parse_pi_session_id_from_path(&path) else {
            continue;
        };
        checked += 1;
        maybe_ingest(
            home,
            user_id,
            &external_id,
            &path,
            workspace_hint.as_deref(),
        )
        .await;
    }
    debug!(
        "jsonl ingest sweep: checked {} session files in {:?}",
        checked,
        started.elapsed()
    );
}

fn list_session_files_with_workspace(home: &Path) -> Vec<(PathBuf, Option<String>)> {
    let base = home.join(".pi/agent/sessions");
    let Ok(workspaces) = std::fs::read_dir(base) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for workspace in workspaces.flatten() {
        let dir = workspace.path();
        if !dir.is_dir() {
            continue;
        }
        let workspace_hint = dir
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(super::server::decode_workspace_path_from_safe_dirname);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
                out.push((path, workspace_hint.clone()));
            }
        }
    }
    out
}

async fn maybe_ingest(
    home: &Path,
    user_id: &str,
    external_id: &str,
    path: &Path,
    workspace_hint: Option<&str>,
) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    let Ok(modified) = meta.modified() else {
        return;
    };
    // Live sessions are fed by the event pipeline; a file this fresh may
    // still be mid-write. It will be picked up once quiescent.
    if modified.elapsed().unwrap_or(Duration::ZERO) < QUIESCENCE {
        return;
    }
    let mtime_ms = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let current = IngestCursor {
        file_size: meta.len() as i64,
        file_mtime_ms: mtime_ms,
    };
    if index::get_ingest_cursor(home, external_id).await == Some(current) {
        return;
    }
    ingest_file(home, user_id, external_id, path, workspace_hint, current).await;
}

async fn ingest_file(
    home: &Path,
    user_id: &str,
    external_id: &str,
    path: &Path,
    workspace_hint: Option<&str>,
    cursor: IngestCursor,
) {
    let path_owned = path.to_path_buf();
    let records = match tokio::task::spawn_blocking(move || {
        super::server::read_jsonl_message_records(&path_owned)
    })
    .await
    {
        Ok(records) => records,
        Err(err) => {
            warn!("jsonl ingest: reading {} failed: {err}", path.display());
            return;
        }
    };

    if records.is_empty() {
        let _ = index::upsert_ingest_cursor(home, external_id, cursor).await;
        return;
    }

    let (session_id, workspace_id) = match ops::find_session_by_external(home, external_id).await {
        Some((id, ws)) if !ws.is_empty() => (id, ws),
        Some((id, _)) => (id, workspace_hint.unwrap_or("global").to_string()),
        None => (
            external_id.to_string(),
            workspace_hint.unwrap_or("global").to_string(),
        ),
    };

    let oqto_log_count = store::read_session_stats(home, &workspace_id, &session_id)
        .await
        .map(|stats| stats.messages)
        .unwrap_or(0);

    if oqto_log_count < records.len() {
        // Converge deterministically on the JSONL contents: the replace is
        // keyed by source entry ids, so re-running it is idempotent.
        if let Err(err) = store::replace_session_with_pi_jsonl_records(
            home,
            user_id,
            &workspace_id,
            &session_id,
            &session_id,
            Some(external_id),
            external_id,
            &records,
        )
        .await
        {
            warn!(
                "jsonl ingest: replace failed session={session_id} external_id={external_id} oqto_log={oqto_log_count} jsonl={}: {err:#}",
                records.len()
            );
            return;
        }
        debug!(
            "jsonl ingest: replaced session={session_id} external_id={external_id} with {} messages (was {oqto_log_count})",
            records.len()
        );
    }

    if let Err(err) = index::upsert_ingest_cursor(home, external_id, cursor).await {
        debug!("jsonl ingest: cursor upsert failed for {external_id}: {err:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_session_file(home: &Path, external_id: &str, messages: &[&str]) -> PathBuf {
        let dir = home.join(".pi/agent/sessions/tmp-ingesttest");
        std::fs::create_dir_all(&dir).expect("create sessions dir");
        let path = dir.join(format!("2026-07-18T12-00-00-000Z_{external_id}.jsonl"));
        let mut lines = vec![format!(
            r#"{{"type":"session","id":"{external_id}","timestamp":"2026-07-18T12:00:00.000Z","cwd":"/tmp/ingesttest"}}"#
        )];
        for (idx, text) in messages.iter().enumerate() {
            let role = if idx % 2 == 0 { "user" } else { "assistant" };
            lines.push(format!(
                r#"{{"type":"message","id":"entry-{idx}","timestamp":"2026-07-18T12:00:01.000Z","message":{{"role":"{role}","content":[{{"type":"text","text":"{text}"}}],"timestamp":1768565213775}}}}"#
            ));
        }
        std::fs::write(&path, lines.join("\n")).expect("write session file");
        // Backdate mtime past the quiescence window.
        let status = std::process::Command::new("touch")
            .arg("-d")
            .arg("2026-07-01T12:00:02Z")
            .arg(&path)
            .status()
            .expect("touch");
        assert!(status.success());
        path
    }

    #[tokio::test]
    async fn sweep_ingests_bare_pi_session_and_sets_cursor() {
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path();
        let external_id = "11111111-2222-3333-4444-555555555555";
        let path = write_session_file(home, external_id, &["hello", "world"]);

        sweep(home, "testuser").await;

        let found = ops::find_session_by_external(home, external_id)
            .await
            .expect("session ingested into oqto-log");
        assert_eq!(found.1, "/tmp/ingesttest");
        let cursor = index::get_ingest_cursor(home, external_id)
            .await
            .expect("cursor recorded");
        assert_eq!(
            cursor.file_size,
            std::fs::metadata(&path).expect("meta").len() as i64
        );

        // Unchanged file: sweep is a no-op (cursor identical).
        sweep(home, "testuser").await;
        assert_eq!(
            index::get_ingest_cursor(home, external_id).await,
            Some(cursor)
        );

        // Out-of-band continuation: more messages appear in the JSONL.
        write_session_file(home, external_id, &["hello", "world", "continued", "again"]);
        sweep(home, "testuser").await;
        let new_cursor = index::get_ingest_cursor(home, external_id)
            .await
            .expect("cursor updated");
        assert!(new_cursor.file_size > cursor.file_size);

        let session_id = ops::find_session_by_external(home, external_id)
            .await
            .expect("session still present")
            .0;
        let messages = oqto_history::oqto_log::projector::project_session_messages_auto(
            home,
            &session_id,
            None,
        )
        .await
        .expect("project")
        .expect("messages present");
        assert_eq!(messages.len(), 4);
    }
}
