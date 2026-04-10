use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::pi::AgentMessage;

use super::store::append_agent_end_snapshot;

#[derive(Debug, Default, Clone)]
pub struct ImportStats {
    pub scanned_files: usize,
    pub imported_sessions: usize,
    pub skipped_files: usize,
    pub failed_files: usize,
    pub imported_messages: usize,
    pub failure_samples: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct JsonlMessageEntry {
    #[serde(rename = "type")]
    entry_type: String,
    message: Option<AgentMessage>,
}

fn decode_workspace_path_from_safe_dirname(dirname: &str) -> Option<String> {
    let trimmed = dirname.trim();
    let core = trimmed
        .strip_prefix("--")
        .and_then(|v| v.strip_suffix("--"))
        .unwrap_or(trimmed);
    if core.is_empty() {
        return None;
    }
    Some(format!("/{}", core.replace('-', "/")))
}

fn parse_pi_session_id_from_path(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy();
    let (_, session_id) = stem.rsplit_once('_')?;
    if session_id.is_empty() {
        None
    } else {
        Some(session_id.to_string())
    }
}

fn read_jsonl_agent_messages(path: &Path) -> Vec<AgentMessage> {
    use std::io::BufRead;

    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };

    let reader = std::io::BufReader::new(file);
    let mut messages = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Ok(entry) = serde_json::from_str::<JsonlMessageEntry>(trimmed) else {
            continue;
        };
        if entry.entry_type != "message" {
            continue;
        }
        if let Some(message) = entry.message {
            messages.push(message);
        }
    }

    messages
}

pub async fn bootstrap_import_from_pi_jsonl(
    user_home: &Path,
    user_id: &str,
) -> Result<ImportStats> {
    let mut stats = ImportStats::default();

    let base = user_home.join(".pi").join("agent").join("sessions");
    let Ok(workspaces) = std::fs::read_dir(base) else {
        return Ok(stats);
    };

    let mut files: Vec<(PathBuf, String)> = Vec::new();
    for workspace in workspaces.flatten() {
        let workspace_dir_path = workspace.path();
        if !workspace_dir_path.is_dir() {
            continue;
        }

        let workspace_id = workspace_dir_path
            .file_name()
            .and_then(|v| v.to_str())
            .and_then(decode_workspace_path_from_safe_dirname)
            .unwrap_or_else(|| "global".to_string());

        let Ok(entries) = std::fs::read_dir(&workspace_dir_path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|v| v.to_str()) == Some("jsonl") {
                files.push((path, workspace_id.clone()));
            }
        }
    }

    files.sort_by(|a, b| a.0.cmp(&b.0));

    for (path, workspace_id) in files {
        stats.scanned_files += 1;
        let Some(session_id) = parse_pi_session_id_from_path(&path) else {
            stats.skipped_files += 1;
            continue;
        };

        let messages = read_jsonl_agent_messages(&path);
        if messages.is_empty() {
            stats.skipped_files += 1;
            continue;
        }

        let last_offset = (messages.len() as i64).saturating_sub(1);
        let mut last_err: Option<anyhow::Error> = None;
        let mut appended = None;
        for _attempt in 0..3 {
            match append_agent_end_snapshot(
                user_home,
                user_id,
                &workspace_id,
                &session_id,
                &session_id,
                Some(&session_id),
                &session_id,
                &messages,
            )
            .await
            {
                Ok(append_stats) => {
                    appended = Some(append_stats);
                    break;
                }
                Err(err) => {
                    last_err = Some(err);
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        }

        if let Some(append_stats) = appended {
            let _ = crate::oqto_log::store::upsert_import_checkpoint(
                user_home,
                &workspace_id,
                "pi_jsonl",
                &session_id,
                &session_id,
                Some(last_offset),
                Some(&format!("entry:{}", last_offset)),
                Some(&append_stats.snapshot_hash),
            )
            .await;
            stats.imported_sessions += 1;
            stats.imported_messages += append_stats.messages_written;
        } else {
            stats.failed_files += 1;
            if stats.failure_samples.len() < 25 {
                let err_text = last_err
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "unknown error".to_string());
                stats.failure_samples.push(format!(
                    "file={} workspace={} session={} error={}",
                    path.display(), workspace_id, session_id, err_text
                ));
            }
        }
    }

    Ok(stats)
}
