use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::pi::AgentMessage;

use super::store::platform_id_for_external_id;
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
struct JsonlSessionEntry {
    #[serde(rename = "type")]
    entry_type: String,
    cwd: Option<String>,
    name: Option<String>,
    timestamp: Option<String>,
    message: Option<serde_json::Value>,
}

#[derive(Debug, Default)]
struct PiJsonlSessionMetadata {
    cwd: Option<String>,
    title: Option<String>,
    readable_id: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

fn parse_pi_jsonl_filename_datetime(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy();
    let (timestamp, _) = stem.rsplit_once('_')?;
    let parsed = chrono::DateTime::parse_from_str(timestamp, "%Y-%m-%dT%H-%M-%S-%3fZ").ok()?;
    Some(parsed.naive_utc().format("%Y-%m-%d %H:%M:%S").to_string())
}

fn millis_to_sqlite_datetime(ms: u64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(ms as i64)
        .map(|dt| dt.naive_utc().format("%Y-%m-%d %H:%M:%S").to_string())
}

fn read_pi_jsonl_session_metadata(path: &Path) -> PiJsonlSessionMetadata {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return PiJsonlSessionMetadata::default();
    };

    let mut cwd = None;
    let mut last_name = None;
    let mut first_timestamp = parse_pi_jsonl_filename_datetime(path);
    let mut last_message_timestamp = None;
    for line in contents.lines() {
        let Ok(entry) = serde_json::from_str::<JsonlSessionEntry>(line) else {
            continue;
        };
        match entry.entry_type.as_str() {
            "session" => {
                if cwd.is_none() {
                    cwd = entry.cwd;
                }
                if first_timestamp.is_none() {
                    first_timestamp = entry.timestamp.and_then(|ts| {
                        chrono::DateTime::parse_from_rfc3339(&ts)
                            .ok()
                            .map(|dt| dt.naive_utc().format("%Y-%m-%d %H:%M:%S").to_string())
                    });
                }
            }
            "message" => {
                if let Some(ms) = entry
                    .message
                    .as_ref()
                    .and_then(|message| message.get("timestamp"))
                    .and_then(|value| {
                        value
                            .as_u64()
                            .or_else(|| value.as_i64().map(|v| v.max(0) as u64))
                    })
                {
                    last_message_timestamp = millis_to_sqlite_datetime(ms);
                }
            }
            "session_info" => {
                if let Some(name) = entry.name.map(|name| name.trim().to_string())
                    && !name.is_empty()
                {
                    last_name = Some(name);
                }
            }
            _ => {}
        }
    }

    let (title, readable_id) = last_name
        .map(|name| {
            let parsed = oqto_pi::session_parser::ParsedTitle::parse(&name);
            let readable_id = parsed.get_readable_id().map(ToOwned::to_owned);
            let display = parsed.display_title().trim();
            let title = if !display.is_empty() {
                Some(display.to_string())
            } else {
                name.split('[')
                    .next()
                    .map(str::trim)
                    .filter(|fallback| !fallback.is_empty())
                    .map(str::to_string)
            };
            (title, readable_id)
        })
        .unwrap_or((None, None));

    PiJsonlSessionMetadata {
        cwd,
        title,
        readable_id,
        created_at: first_timestamp.clone(),
        updated_at: last_message_timestamp.or(first_timestamp),
    }
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
    _workspace_id: &str,
    session_id: &str,
) -> Option<PathBuf> {
    let base = user_home.join(".pi").join("agent").join("sessions");
    let Ok(workspaces) = std::fs::read_dir(base) else {
        return None;
    };

    // Match by session id only: the safe dirname encoding is lossy for
    // paths containing '-', so a decoded-dirname == workspace filter would
    // wrongly skip hyphenated workspaces. Session ids are globally unique.
    for workspace in workspaces.flatten() {
        let workspace_dir_path = workspace.path();
        if !workspace_dir_path.is_dir() {
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

/// Converge a documented pre-binding identity split (oqto-9np8) with the Pi
/// JSONL as authority: exact-replace into the canonical public id, which
/// removes the competing legacy rows in the same transaction. Without JSONL
/// evidence the conflict stays fail-closed and is reported instead.
async fn repair_legacy_identity_conflict(
    user_home: &Path,
    user_id: &str,
    workspace_id: &str,
    conflict: &oqto_history::oqto_log::ops::SessionIdentityConflict,
    jsonl_path: Option<&Path>,
) -> Result<()> {
    let path = jsonl_path.context("no Pi JSONL found for conflicted external id")?;
    let records = read_jsonl_message_records(path);
    if records.is_empty() {
        anyhow::bail!(
            "conflicted session JSONL has no messages; refusing to repair without authority"
        );
    }
    let target = platform_id_for_external_id(&conflict.external_id);
    oqto_history::oqto_log::store::replace_session_with_pi_jsonl_records(
        user_home,
        user_id,
        workspace_id,
        &target,
        &target,
        Some(&conflict.external_id),
        &conflict.external_id,
        &records,
    )
    .await
    .context("exact-replace repair for conflicted identity")?;
    Ok(())
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
    let mut jsonl_by_external: std::collections::BTreeMap<String, std::path::PathBuf> =
        std::collections::BTreeMap::new();

    for workspace in workspaces.flatten() {
        let workspace_dir_path = workspace.path();
        if !workspace_dir_path.is_dir() {
            continue;
        }

        let fallback_workspace_id = workspace_dir_path
            .file_name()
            .and_then(|v| v.to_str())
            .and_then(decode_workspace_path_from_safe_dirname)
            .unwrap_or_else(|| "global".to_string());

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
            let metadata = read_pi_jsonl_session_metadata(&path);
            let workspace_id = metadata
                .cwd
                .unwrap_or_else(|| fallback_workspace_id.clone());
            if let Some(root) = workspace_root
                && !path_is_inside_root(&workspace_id, root)
            {
                stats.skipped_files += 1;
                continue;
            }
            jsonl_by_external.insert(external_id.clone(), path.clone());
            by_workspace.entry(workspace_id.clone()).or_default().push(
                oqto_history::oqto_log::ops::SessionIdentityInput {
                    platform_id: platform_id_for_external_id(&external_id),
                    external_id,
                    title: metadata.title,
                    readable_id: metadata.readable_id,
                    created_at: metadata.created_at,
                    updated_at: metadata.updated_at,
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
            Ok(outcome) => {
                stats.imported_sessions += outcome.upserted;
                for conflict in outcome.conflicts {
                    match repair_legacy_identity_conflict(
                        user_home,
                        user_id,
                        &workspace_id,
                        &conflict,
                        jsonl_by_external
                            .get(&conflict.external_id)
                            .map(std::path::PathBuf::as_path),
                    )
                    .await
                    {
                        Ok(()) => stats.imported_sessions += 1,
                        Err(err) => {
                            stats.failed_files += 1;
                            if stats.failure_samples.len() < 25 {
                                stats.failure_samples.push(format!(
                                    "identity_conflict_unrepaired workspace={} external_id={} sessions={:?} error={}",
                                    workspace_id, conflict.external_id, conflict.session_ids, err
                                ));
                            }
                        }
                    }
                }
            }
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
    import_from_pi_jsonl(user_home, user_id, false).await
}

/// Re-project every Pi JSONL from authority, ignoring incremental fingerprints.
/// This is the explicit repair path for historical split/over-populated rows;
/// ordinary deploys continue to use incremental bootstrap.
pub async fn rebuild_from_pi_jsonl(user_home: &Path, user_id: &str) -> Result<ImportStats> {
    import_from_pi_jsonl(user_home, user_id, true).await
}

async fn import_from_pi_jsonl(
    user_home: &Path,
    user_id: &str,
    force_rebuild: bool,
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

        let fallback_workspace_id = workspace_dir_path
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
                files.push((path, fallback_workspace_id.clone()));
            }
        }
    }

    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut imported_sessions_this_run: Vec<ImportedSession> = Vec::new();

    for (path, fallback_workspace_id) in files {
        stats.scanned_files += 1;
        let path_key = path.to_string_lossy().to_string();
        let current_fp = file_fingerprint(&path);
        if !force_rebuild
            && let (Some(current), Some(previous)) =
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

        // The JSONL header cwd is the exact workspace; the safe dirname
        // decode is lossy for paths containing '-'.
        let workspace_id = read_pi_jsonl_session_metadata(&path)
            .cwd
            .unwrap_or(fallback_workspace_id);

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
                None => (
                    oqto_history::oqto_log::store::platform_id_for_external_id(&pi_session_id),
                    workspace_id,
                ),
            };

        let last_offset = (messages.len() as i64).saturating_sub(1);
        let mut last_err: Option<anyhow::Error> = None;
        let mut appended = None;
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
            // Exact replace is the only bootstrap write mode. It converges
            // under- and over-populated projections to JSONL truth and repairs
            // same-store identity splits transactionally before source tuples
            // are inserted. The reported id is the identity actually written.
            let session_id = append_stats.session_id.clone();

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
            let final_fp = file_fingerprint(&path);
            if current_fp.is_some() && current_fp == final_fp {
                if let Some(fp) = final_fp {
                    importer_state.files.insert(path_key.clone(), fp);
                }
                // Validation keys sessions by the harness id parsed from the
                // JSONL filename. Only stable files enter the deploy gate: an
                // active Pi process may append while bootstrap reads it, and a
                // moving source cannot have an exact point-in-time count.
                imported_sessions_this_run.push(ImportedSession {
                    workspace_id: workspace_id.clone(),
                    session_id: pi_session_id.clone(),
                });
            } else {
                importer_state.files.remove(&path_key);
            }
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

    // Post-pass repair only the stable sessions touched by this invocation.
    // Validating all ~2.5k JSONLs made a five-session deploy pass take minutes
    // and raced active files that necessarily changed during the scan.
    let stable_filter: std::collections::HashSet<(String, String)> = imported_sessions_this_run
        .iter()
        .map(|session| (session.workspace_id.clone(), session.session_id.clone()))
        .collect();
    if !stable_filter.is_empty()
        && let Ok(report) = crate::oqto_log::validator::validate_bootstrap_import_filtered(
            user_home,
            Some(&stable_filter),
        )
        .await
        && report.sessions_mismatch > 0
    {
        for row in report.mismatches {
            let Some((workspace_id, mismatch_session_id)) = parse_mismatch_row(&row) else {
                continue;
            };

            // Validation keys by Pi external id. Select the canonical row
            // deterministically; exact replace removes same-store competitors
            // in its own rollback-safe transaction.
            let target_external_id = mismatch_session_id.clone();
            let candidates = oqto_history::oqto_log::ops::list_sessions_by_external_in_workspace(
                user_home,
                &workspace_id,
                &target_external_id,
            )
            .await
            .unwrap_or_default();
            let target_session_id = candidates.first().cloned().unwrap_or_else(|| {
                oqto_history::oqto_log::store::platform_id_for_external_id(&target_external_id)
            });
            let Some(path) = find_session_jsonl_path(user_home, &workspace_id, &target_external_id)
            else {
                continue;
            };
            let path_key = path.to_string_lossy().to_string();
            if file_fingerprint(&path) != importer_state.files.get(&path_key).cloned() {
                // Source moved after import; background ingest or the next
                // deploy pass will converge it once quiescent.
                importer_state.files.remove(&path_key);
                continue;
            }
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
                    &target_session_id,
                    &target_session_id,
                    Some(&target_external_id),
                    &target_external_id,
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
                        "postpass_replace_failed workspace={} session={} canonical={} external={} error={}",
                        workspace_id, mismatch_session_id, target_session_id, target_external_id, err_text
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_jsonl_metadata_uses_jsonl_cwd_title_and_source_timestamps() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("2026-01-16T12-06-53-369Z_session-1.jsonl");
        std::fs::write(
            &path,
            concat!(
                r#"{"type":"session","timestamp":"2026-01-16T12:06:53.369Z","cwd":"/home/wismut/.pi/agent/extensions"}"#,
                "\n",
                r#"{"type":"message","message":{"role":"user","content":[{"type":"text","text":"hi"}],"timestamp":1770410509191}}"#,
                "\n",
                r#"{"type":"session_info","name":"Fix extension loader [owl-123]"}"#,
                "\n"
            ),
        )
        .expect("write jsonl");

        let metadata = read_pi_jsonl_session_metadata(&path);
        assert_eq!(
            metadata.cwd.as_deref(),
            Some("/home/wismut/.pi/agent/extensions")
        );
        assert_eq!(metadata.title.as_deref(), Some("Fix extension loader"));
        assert_eq!(metadata.created_at.as_deref(), Some("2026-01-16 12:06:53"));
        assert_eq!(metadata.updated_at.as_deref(), Some("2026-02-06 20:41:49"));
    }

    /// Validation treats recently flushed JSONLs as live runtime drift, so
    /// fixtures must look quiescent before strict count assertions apply.
    fn age_jsonl(path: &std::path::Path) {
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        let file = std::fs::OpenOptions::new()
            .append(true)
            .open(path)
            .expect("open jsonl for aging");
        file.set_times(std::fs::FileTimes::new().set_modified(old))
            .expect("age jsonl mtime");
    }

    #[tokio::test]
    async fn validation_treats_freshly_flushed_sessions_as_live_not_corrupt() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = "/tmp/oqto-importer-live";
        let pi_id = "019fae41-0000-7000-a000-000000000001";
        let sessions_dir = temp
            .path()
            .join(".pi/agent/sessions/--tmp-oqto-importer-live--");
        std::fs::create_dir_all(&sessions_dir).expect("create Pi session dir");
        let jsonl = sessions_dir.join(format!("2026-08-18T00-00-00-000Z_{pi_id}.jsonl"));
        std::fs::write(
            &jsonl,
            format!(
                "{{\"type\":\"session\",\"cwd\":\"{workspace}\"}}\n{{\"type\":\"message\",\"message\":{{\"role\":\"user\",\"content\":\"one\"}}}}\n"
            ),
        )
        .expect("write Pi JSONL");
        age_jsonl(&jsonl);
        bootstrap_import_from_pi_jsonl(temp.path(), "user-1")
            .await
            .expect("initial import");

        let db_path = oqto_history::oqto_log::paths::resolve_user_home_workspace_db_path(
            temp.path(),
            workspace,
        )
        .expect("db path");
        let pool = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(&db_path),
        )
        .await
        .expect("open oqto-log");
        let session_id: String =
            sqlx::query_scalar("SELECT session_id FROM oqto_log_sessions WHERE external_id = ?")
                .bind(pi_id)
                .fetch_one(&pool)
                .await
                .expect("session id");
        let branch_id = format!("branch:{session_id}:main");
        sqlx::query(
            "INSERT INTO oqto_log_turns (turn_id, session_id, branch_id, turn_version, role, status) VALUES ('live-turn', ?, ?, 3, 'assistant', 'committed')",
        )
        .bind(&session_id)
        .bind(&branch_id)
        .execute(&pool)
        .await
        .expect("seed runtime turn");
        sqlx::query(
            "INSERT INTO oqto_log_messages (message_id, turn_id, seq, kind, role, content) VALUES ('live-message', 'live-turn', 0, 'text', 'assistant', 'streamed ahead of flush')",
        )
        .execute(&pool)
        .await
        .expect("seed runtime message");
        pool.close().await;

        // A just-flushed JSONL marks the Session as live: divergence is
        // runtime drift, not corruption.
        std::fs::OpenOptions::new()
            .append(true)
            .open(&jsonl)
            .and_then(|mut file| std::io::Write::write_all(&mut file, b"\n"))
            .expect("simulate fresh flush");
        let filter = std::collections::HashSet::from([(workspace.to_string(), pi_id.to_string())]);
        let live = crate::oqto_log::validator::validate_bootstrap_import_filtered(
            temp.path(),
            Some(&filter),
        )
        .await
        .expect("validate live session");
        assert_eq!(live.sessions_mismatch, 0, "live drift must not fail deploy");
        assert_eq!(
            live.sessions_unstable, 1,
            "live session must be reported as unstable"
        );

        // The same divergence on a quiescent Session is corruption.
        age_jsonl(&jsonl);
        let quiescent = crate::oqto_log::validator::validate_bootstrap_import_filtered(
            temp.path(),
            Some(&filter),
        )
        .await
        .expect("validate quiescent session");
        assert_eq!(quiescent.sessions_mismatch, 1);
        assert_eq!(quiescent.sessions_unstable, 0);
    }

    #[tokio::test]
    async fn identity_sync_repairs_prebinding_split_from_jsonl_authority() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = "/tmp/oqto-importer-split";
        let pi_id = "dc7f6065-f8e4-43f7-8b14-c44e7fa179b8";
        let sessions_dir = temp
            .path()
            .join(".pi/agent/sessions/--tmp-oqto-importer-split--");
        std::fs::create_dir_all(&sessions_dir).expect("create Pi session dir");
        let jsonl = sessions_dir.join(format!("2026-08-18T00-00-00-000Z_{pi_id}.jsonl"));
        std::fs::write(
            &jsonl,
            format!(
                "{{\"type\":\"session\",\"cwd\":\"{workspace}\"}}\n{{\"type\":\"message\",\"message\":{{\"role\":\"user\",\"content\":\"one\"}}}}\n{{\"type\":\"message\",\"message\":{{\"role\":\"assistant\",\"content\":\"two\"}}}}\n"
            ),
        )
        .expect("write Pi JSONL");

        // Seed the archvm oqto-svwp shape: an empty canonical row and a raw
        // self-identified row both claim the Pi id; no binding facts exist.
        let db_path = oqto_history::oqto_log::paths::resolve_user_home_workspace_db_path(
            temp.path(),
            workspace,
        )
        .expect("db path");
        oqto_history::oqto_log::store::migrate_db_path(&db_path)
            .await
            .expect("migrate");
        let pool = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(&db_path),
        )
        .await
        .expect("open oqto-log");
        for (session_id, platform_id) in [
            ("oqto-canonical-empty", "oqto-canonical-empty"),
            (pi_id, pi_id),
        ] {
            sqlx::query(
                "INSERT INTO oqto_log_sessions (session_id, platform_id, external_id, user_id, workspace_id) VALUES (?, ?, ?, 'user-1', ?)",
            )
            .bind(session_id)
            .bind(platform_id)
            .bind(pi_id)
            .bind(workspace)
            .execute(&pool)
            .await
            .expect("seed split row");
        }

        let stats = fast_import_identities_from_pi_jsonl(temp.path(), "user-1", None)
            .await
            .expect("identity sync");
        assert_eq!(
            stats.failed_files, 0,
            "split must be repaired, not reported: {:?}",
            stats.failure_samples
        );

        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT session_id, platform_id FROM oqto_log_sessions WHERE external_id = ? ORDER BY session_id",
        )
        .bind(pi_id)
        .fetch_all(&pool)
        .await
        .expect("rows after repair");
        assert_eq!(
            rows.len(),
            1,
            "exactly one identity row must remain: {rows:?}"
        );
        let (session_id, platform_id) = &rows[0];
        assert!(
            session_id.starts_with("oqto-"),
            "public id must be canonical: {session_id}"
        );
        assert_eq!(session_id, platform_id);

        let messages: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM oqto_log_messages m JOIN oqto_log_turns t ON t.turn_id = m.turn_id WHERE t.session_id = ?",
        )
        .bind(session_id)
        .fetch_one(&pool)
        .await
        .expect("messages after repair");
        assert_eq!(messages, 2, "canonical session must carry the JSONL truth");
    }

    #[tokio::test]
    async fn bootstrap_replaces_overpopulated_projection_with_exact_jsonl_truth() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = "/tmp/oqto-importer-exact";
        let pi_id = "019f5f22-3777-78b7-ab0e-539863fb8232";
        let sessions_dir = temp
            .path()
            .join(".pi/agent/sessions/--tmp-oqto-importer-exact--");
        std::fs::create_dir_all(&sessions_dir).expect("create Pi session dir");
        let jsonl = sessions_dir.join(format!("2026-08-18T00-00-00-000Z_{pi_id}.jsonl"));
        std::fs::write(
            &jsonl,
            format!(
                "{{\"type\":\"session\",\"cwd\":\"{workspace}\"}}\n{{\"type\":\"message\",\"message\":{{\"role\":\"user\",\"content\":\"one\"}}}}\n{{\"type\":\"message\",\"message\":{{\"role\":\"assistant\",\"content\":\"two\"}}}}\n"
            ),
        )
        .expect("write Pi JSONL");
        age_jsonl(&jsonl);

        bootstrap_import_from_pi_jsonl(temp.path(), "user-1")
            .await
            .expect("initial exact import");
        let db_path = oqto_history::oqto_log::paths::resolve_user_home_workspace_db_path(
            temp.path(),
            workspace,
        )
        .expect("db path");
        let pool = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(&db_path),
        )
        .await
        .expect("open oqto-log");
        let session_id: String =
            sqlx::query_scalar("SELECT session_id FROM oqto_log_sessions WHERE external_id = ?")
                .bind(pi_id)
                .fetch_one(&pool)
                .await
                .expect("session id");
        let branch_id = format!("branch:{session_id}:main");
        sqlx::query(
            "INSERT INTO oqto_log_turns (turn_id, session_id, branch_id, turn_version, role, status) VALUES ('extra-turn', ?, ?, 3, 'assistant', 'committed')",
        )
        .bind(&session_id)
        .bind(&branch_id)
        .execute(&pool)
        .await
        .expect("seed overpopulated turn");
        sqlx::query(
            "INSERT INTO oqto_log_messages (message_id, turn_id, seq, kind, role, content) VALUES ('extra-message', 'extra-turn', 0, 'text', 'assistant', 'duplicate')",
        )
        .execute(&pool)
        .await
        .expect("seed overpopulated message");
        pool.close().await;

        let filter = std::collections::HashSet::from([(workspace.to_string(), pi_id.to_string())]);
        let report = crate::oqto_log::validator::validate_bootstrap_import_filtered(
            temp.path(),
            Some(&filter),
        )
        .await
        .expect("validate overpopulation");
        assert_eq!(
            report.sessions_mismatch, 1,
            "overpopulation must fail validation"
        );

        // Size changes even when the filesystem timestamp has one-second
        // granularity, so the incremental importer must revisit this source.
        std::fs::OpenOptions::new()
            .append(true)
            .open(&jsonl)
            .and_then(|mut file| std::io::Write::write_all(&mut file, b"\n"))
            .expect("touch JSONL fingerprint");
        age_jsonl(&jsonl);
        bootstrap_import_from_pi_jsonl(temp.path(), "user-1")
            .await
            .expect("repair import");

        let pool = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(&db_path),
        )
        .await
        .expect("reopen oqto-log");
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM oqto_log_messages m JOIN oqto_log_turns t ON t.turn_id = m.turn_id WHERE t.session_id = ?",
        )
        .bind(&session_id)
        .fetch_one(&pool)
        .await
        .expect("count repaired messages");
        assert_eq!(count, 2, "bootstrap must converge to exact JSONL truth");

        sqlx::query(
            "INSERT INTO oqto_log_turns (turn_id, session_id, branch_id, turn_version, role, status) VALUES ('extra-turn-2', ?, ?, 3, 'assistant', 'committed')",
        )
        .bind(&session_id)
        .bind(&branch_id)
        .execute(&pool)
        .await
        .expect("seed second overpopulation");
        sqlx::query(
            "INSERT INTO oqto_log_messages (message_id, turn_id, seq, kind, role, content) VALUES ('extra-message-2', 'extra-turn-2', 0, 'text', 'assistant', 'duplicate again')",
        )
        .execute(&pool)
        .await
        .expect("seed second duplicate");
        pool.close().await;

        let incremental = bootstrap_import_from_pi_jsonl(temp.path(), "user-1")
            .await
            .expect("incremental bootstrap");
        assert_eq!(incremental.imported_sessions, 0);
        assert_eq!(incremental.skipped_files, 1);

        rebuild_from_pi_jsonl(temp.path(), "user-1")
            .await
            .expect("forced authoritative rebuild");
        let pool = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(&db_path),
        )
        .await
        .expect("reopen rebuilt oqto-log");
        let rebuilt_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM oqto_log_messages m JOIN oqto_log_turns t ON t.turn_id = m.turn_id WHERE t.session_id = ?",
        )
        .bind(&session_id)
        .fetch_one(&pool)
        .await
        .expect("count authoritatively rebuilt messages");
        assert_eq!(rebuilt_count, 2, "forced rebuild must ignore fingerprints");
    }

    #[test]
    fn safe_dir_decoder_is_only_a_fallback_for_missing_jsonl_cwd() {
        assert_eq!(
            decode_workspace_path_from_safe_dirname("--home-wismut-byteowlz-pi-agent-extensions--")
                .as_deref(),
            Some("/home/wismut/byteowlz/pi/agent/extensions")
        );

        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("2026-02-06T20-41-29-820Z_session-2.jsonl");
        std::fs::write(
            &path,
            r#"{"type":"session","timestamp":"2026-02-06T20:41:29.820Z","cwd":"/home/wismut/byteowlz/pi-agent-extensions"}"#,
        )
        .expect("write jsonl");

        let metadata = read_pi_jsonl_session_metadata(&path);
        assert_eq!(
            metadata.cwd.as_deref(),
            Some("/home/wismut/byteowlz/pi-agent-extensions")
        );
    }
}
