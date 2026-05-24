use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::pi::AgentMessage;

use super::store::append_agent_end_snapshot;
use oqto_history::oqto_log::store::PiJsonlMessageRecord;

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
    id: Option<String>,
    #[serde(rename = "parentId")]
    parent_id: Option<String>,
    message: Option<serde_json::Value>,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct ImporterState {
    files: HashMap<String, FileFingerprint>,
    last_imported_sessions: Vec<ImportedSession>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ImportedSession {
    workspace_id: String,
    session_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct FileFingerprint {
    mtime_secs: u64,
    size: u64,
}

fn importer_state_path(user_home: &Path) -> PathBuf {
    user_home
        .join(".local")
        .join("share")
        .join("oqto")
        .join("oqto-log")
        .join("importer-state.json")
}

fn load_importer_state(user_home: &Path) -> ImporterState {
    let path = importer_state_path(user_home);
    if let Ok(raw) = std::fs::read_to_string(path) {
        serde_json::from_str::<ImporterState>(&raw).unwrap_or_default()
    } else {
        ImporterState::default()
    }
}

fn save_importer_state(user_home: &Path, state: &ImporterState) {
    let path = importer_state_path(user_home);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(serialized) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(path, serialized);
    }
}

fn file_fingerprint(path: &Path) -> Option<FileFingerprint> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let mtime_secs = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(FileFingerprint {
        mtime_secs,
        size: meta.len(),
    })
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

fn path_is_inside_root(path: &str, root: &Path) -> bool {
    let normalized_path = path.replace('\\', "/").trim_end_matches('/').to_string();
    let normalized_root = root
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    normalized_path == normalized_root
        || normalized_path.starts_with(&format!("{normalized_root}/"))
}

fn platform_id_for_external_id(external_id: &str) -> String {
    const NS: uuid::Uuid = uuid::uuid!("7a0b6c2e-74b2-4d2f-a4d3-6d5f7a9d1c31");
    format!("oqto-{}", uuid::Uuid::new_v5(&NS, external_id.as_bytes()))
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

fn parse_agent_message(value: serde_json::Value) -> Option<AgentMessage> {
    if let Ok(parsed) = serde_json::from_value::<AgentMessage>(value.clone()) {
        return Some(parsed);
    }

    let obj = value.as_object()?;
    let role = obj
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("assistant")
        .to_string();

    let content = obj
        .get("content")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));

    let timestamp = obj
        .get("timestamp")
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|x| x.max(0) as u64)));

    let tool_call_id = obj
        .get("toolCallId")
        .or_else(|| obj.get("tool_call_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let tool_name = obj
        .get("toolName")
        .or_else(|| obj.get("tool_name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let is_error = obj
        .get("isError")
        .or_else(|| obj.get("is_error"))
        .and_then(|v| v.as_bool());

    Some(AgentMessage {
        role,
        content,
        timestamp,
        tool_call_id,
        tool_name,
        is_error,
        api: obj
            .get("api")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        provider: obj
            .get("provider")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        model: obj
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        usage: None,
        stop_reason: obj
            .get("stopReason")
            .or_else(|| obj.get("stop_reason"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        extra: std::collections::HashMap::new(),
    })
}

fn read_jsonl_message_records(path: &Path) -> Vec<PiJsonlMessageRecord> {
    use std::io::BufRead;

    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };

    let reader = std::io::BufReader::new(file);
    let mut records = Vec::new();

    for (line_idx, line) in reader.lines().map_while(Result::ok).enumerate() {
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
        if let Some(message_value) = entry.message
            && let Some(message) = parse_agent_message(message_value)
        {
            records.push(PiJsonlMessageRecord {
                source_entry_id: entry.id.unwrap_or_else(|| format!("line:{line_idx}")),
                parent_source_entry_id: entry.parent_id,
                source_sequence: line_idx as i64,
                message,
            });
        }
    }

    records
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
        if let Some(message_value) = entry.message
            && let Some(message) = parse_agent_message(message_value)
        {
            messages.push(message);
        }
    }

    messages
}

fn find_session_jsonl_path(
    user_home: &Path,
    workspace_id: &str,
    session_id: &str,
) -> Option<PathBuf> {
    let base = user_home.join(".pi").join("agent").join("sessions");
    let Ok(workspaces) = std::fs::read_dir(base) else {
        return None;
    };

    for workspace in workspaces.flatten() {
        let workspace_dir_path = workspace.path();
        if !workspace_dir_path.is_dir() {
            continue;
        }

        let decoded = workspace_dir_path
            .file_name()
            .and_then(|v| v.to_str())
            .and_then(decode_workspace_path_from_safe_dirname)
            .unwrap_or_else(|| "global".to_string());
        if decoded != workspace_id {
            continue;
        }

        let Ok(entries) = std::fs::read_dir(&workspace_dir_path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|v| v.to_str()) != Some("jsonl") {
                continue;
            }
            if parse_pi_session_id_from_path(&path).as_deref() == Some(session_id) {
                return Some(path);
            }
        }
    }

    None
}

fn parse_mismatch_row(row: &str) -> Option<(String, String)> {
    // format: workspace=... session=... jsonl_messages=... oqto_log_messages=...
    let mut workspace: Option<String> = None;
    let mut session: Option<String> = None;
    for token in row.split_whitespace() {
        if let Some(v) = token.strip_prefix("workspace=") {
            workspace = Some(v.to_string());
        } else if let Some(v) = token.strip_prefix("session=") {
            session = Some(v.to_string());
        }
    }
    match (workspace, session) {
        (Some(w), Some(s)) => Some((w, s)),
        _ => None,
    }
}

pub async fn fast_import_identities_from_pi_jsonl(
    user_home: &Path,
    user_id: &str,
    workspace_root: Option<&Path>,
) -> Result<ImportStats> {
    let mut stats = ImportStats::default();
    let base = user_home.join(".pi").join("agent").join("sessions");
    let Ok(workspaces) = std::fs::read_dir(base) else {
        return Ok(stats);
    };

    let mut by_workspace: std::collections::BTreeMap<
        String,
        Vec<oqto_history::oqto_log::ops::SessionIdentityInput>,
    > = std::collections::BTreeMap::new();

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
        if let Some(root) = workspace_root
            && !path_is_inside_root(&workspace_id, root)
        {
            continue;
        }

        let Ok(entries) = std::fs::read_dir(&workspace_dir_path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|v| v.to_str()) != Some("jsonl") {
                continue;
            }
            stats.scanned_files += 1;
            let Some(external_id) = parse_pi_session_id_from_path(&path) else {
                stats.skipped_files += 1;
                continue;
            };
            by_workspace.entry(workspace_id.clone()).or_default().push(
                oqto_history::oqto_log::ops::SessionIdentityInput {
                    platform_id: platform_id_for_external_id(&external_id),
                    external_id,
                },
            );
        }
    }

    for (workspace_id, identities) in by_workspace {
        match oqto_history::oqto_log::ops::batch_upsert_session_identities(
            user_home,
            user_id,
            &workspace_id,
            &identities,
        )
        .await
        {
            Ok(imported) => stats.imported_sessions += imported,
            Err(err) => {
                stats.failed_files += identities.len();
                if stats.failure_samples.len() < 25 {
                    stats.failure_samples.push(format!(
                        "identity_batch_failed workspace={} sessions={} error={}",
                        workspace_id,
                        identities.len(),
                        err
                    ));
                }
            }
        }
    }

    Ok(stats)
}

pub async fn bootstrap_import_from_pi_jsonl(
    user_home: &Path,
    user_id: &str,
) -> Result<ImportStats> {
    let mut stats = ImportStats::default();
    let mut importer_state = load_importer_state(user_home);

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

    let mut imported_sessions_this_run: Vec<ImportedSession> = Vec::new();

    for (path, workspace_id) in files {
        stats.scanned_files += 1;
        let path_key = path.to_string_lossy().to_string();
        let current_fp = file_fingerprint(&path);
        if let (Some(current), Some(previous)) =
            (current_fp.as_ref(), importer_state.files.get(&path_key))
            && current.mtime_secs == previous.mtime_secs
            && current.size == previous.size
        {
            stats.skipped_files += 1;
            continue;
        }

        let Some(pi_session_id) = parse_pi_session_id_from_path(&path) else {
            stats.skipped_files += 1;
            continue;
        };

        let records = read_jsonl_message_records(&path);
        let messages: Vec<AgentMessage> = records.iter().map(|r| r.message.clone()).collect();
        if messages.is_empty() {
            stats.skipped_files += 1;
            continue;
        }

        // If an oqto-log session already exists with this Pi ID as its
        // external_id (created at runtime under an oqto-* session_id),
        // merge into that session instead of creating a duplicate.
        // Also use the workspace_id from the existing session to ensure we
        // write to the correct database file.
        let (session_id, workspace_id) =
            match oqto_history::oqto_log::ops::find_session_by_external(user_home, &pi_session_id)
                .await
            {
                Some((existing_id, existing_ws)) if !existing_ws.is_empty() => {
                    (existing_id, existing_ws)
                }
                Some((existing_id, _)) => (existing_id, workspace_id),
                None => (pi_session_id.clone(), workspace_id),
            };

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
                Some(&pi_session_id),
                &pi_session_id,
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

        if let Some(mut append_stats) = appended {
            if let Ok(sess_stats) = oqto_history::oqto_log::store::read_session_stats(
                user_home,
                &workspace_id,
                &session_id,
            )
            .await
                && sess_stats.messages < messages.len()
            {
                // Self-heal partial historical sessions by replacing with the
                // exact JSONL snapshot deterministically.
                let mut replaced_ok = None;
                let mut replace_err: Option<anyhow::Error> = None;
                for _attempt in 0..3 {
                    match oqto_history::oqto_log::store::replace_session_with_pi_jsonl_records(
                        user_home,
                        user_id,
                        &workspace_id,
                        &session_id,
                        &session_id,
                        Some(&pi_session_id),
                        &pi_session_id,
                        &records,
                    )
                    .await
                    {
                        Ok(replaced) => {
                            replaced_ok = Some(replaced);
                            break;
                        }
                        Err(err) => {
                            replace_err = Some(err);
                            tokio::time::sleep(std::time::Duration::from_millis(75)).await;
                        }
                    }
                }

                if let Some(replaced) = replaced_ok {
                    append_stats = replaced;
                } else {
                    let err_text = replace_err
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "unknown replace error".to_string());
                    stats.failed_files += 1;
                    if stats.failure_samples.len() < 25 {
                        stats.failure_samples.push(format!(
                            "replace_failed workspace={} session={} error={}",
                            workspace_id, session_id, err_text
                        ));
                    }
                }
            }

            let _ = oqto_history::oqto_log::store::upsert_import_checkpoint(
                user_home,
                &workspace_id,
                "pi_jsonl",
                &pi_session_id,
                &session_id,
                Some(last_offset),
                Some(&format!("entry:{}", last_offset)),
                Some(&append_stats.snapshot_hash),
            )
            .await;
            if let Some(fp) = current_fp.clone() {
                importer_state.files.insert(path_key, fp);
            }
            imported_sessions_this_run.push(ImportedSession {
                workspace_id: workspace_id.clone(),
                session_id: session_id.clone(),
            });
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
                    path.display(),
                    workspace_id,
                    session_id,
                    err_text
                ));
            }
        }
    }

    // Post-pass repair: if validator still reports mismatches, force-replace
    // those sessions from JSONL to guarantee deploy gate consistency.
    if let Ok(report) = crate::oqto_log::validator::validate_bootstrap_import(user_home).await
        && report.sessions_mismatch > 0
    {
        for row in report.mismatches {
            let Some((workspace_id, session_id)) = parse_mismatch_row(&row) else {
                continue;
            };
            let Some(path) = find_session_jsonl_path(user_home, &workspace_id, &session_id) else {
                continue;
            };
            let records = read_jsonl_message_records(&path);
            if records.is_empty() {
                continue;
            }
            let mut replaced_ok = None;
            let mut replace_err: Option<anyhow::Error> = None;
            for _attempt in 0..3 {
                match oqto_history::oqto_log::store::replace_session_with_pi_jsonl_records(
                    user_home,
                    user_id,
                    &workspace_id,
                    &session_id,
                    &session_id,
                    Some(&session_id),
                    &session_id,
                    &records,
                )
                .await
                {
                    Ok(replaced) => {
                        replaced_ok = Some(replaced);
                        break;
                    }
                    Err(err) => {
                        replace_err = Some(err);
                        tokio::time::sleep(std::time::Duration::from_millis(75)).await;
                    }
                }
            }

            if let Some(replaced) = replaced_ok {
                stats.imported_messages += replaced.messages_written;
            } else {
                let err_text = replace_err
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "unknown replace error".to_string());
                stats.failed_files += 1;
                if stats.failure_samples.len() < 25 {
                    stats.failure_samples.push(format!(
                        "postpass_replace_failed workspace={} session={} error={}",
                        workspace_id, session_id, err_text
                    ));
                }
            }
        }
    }

    importer_state.last_imported_sessions = imported_sessions_this_run;
    save_importer_state(user_home, &importer_state);

    if stats
        .failure_samples
        .iter()
        .any(|s| s.contains("replace_failed") || s.contains("postpass_replace_failed"))
    {
        anyhow::bail!(
            "oqto-log bootstrap repair failed for one or more sessions: {}",
            stats.failure_samples.join(" | ")
        );
    }

    Ok(stats)
}
