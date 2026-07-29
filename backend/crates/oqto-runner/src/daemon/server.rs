use anyhow::{Context, Result};
use chrono::TimeZone;
use log::{debug, error, info, warn};
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::process::Command;
use tokio::sync::{Mutex, RwLock, broadcast};

use crate::daemon::config::RunnerUserConfig;
use crate::daemon::state::{ManagedProcess, RunnerState, SessionState, StdoutBuffer, StdoutEvent};
use crate::pi_manager::PiSessionManager;
use crate::protocol::*;
use crate::transport::{RunnerListener, UnixRunnerListener};
use crate::wire::{encode_json_frame, read_json_frame};
use oqto_sandbox::SandboxConfig;

mod handlers;

/// Configuration for session service binaries.
#[derive(Debug, Clone)]
pub struct SessionBinaries {
    pub fileserver: String,
    pub ttyd: String,
}

/// The runner daemon.
pub struct Runner {
    state: Arc<RwLock<RunnerState>>,
    shutdown_tx: broadcast::Sender<()>,
    /// Sandbox configuration (loaded from trusted system config).
    sandbox_config: Option<SandboxConfig>,
    /// Session service binary paths.
    binaries: SessionBinaries,
    /// User configuration (paths, etc.)
    user_config: RunnerUserConfig,
    /// Pi session manager (manages Pi agent processes).
    pi_manager: Arc<PiSessionManager>,
    /// Background Pi JSONL -> oqto-log ingest task.
    jsonl_ingest: super::jsonl_ingest::IngestHandle,
}

/// Basic-auth username for the session terminal.
pub const TERMINAL_USERNAME: &str = "oqto";

/// Build the ttyd argument vector for a session terminal.
pub fn build_ttyd_args(port: u16, cwd: &str, password: &str) -> Vec<String> {
    vec![
        "--port".to_string(),
        port.to_string(),
        "--interface".to_string(),
        "127.0.0.1".to_string(),
        "--credential".to_string(),
        format!("{}:{}", TERMINAL_USERNAME, password),
        "--check-origin".to_string(),
        "--writable".to_string(),
        "--cwd".to_string(),
        cwd.to_string(),
        "zsh".to_string(),
        "-l".to_string(),
    ]
}

/// Fresh per session start.
fn generate_terminal_credential() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[derive(Debug, serde::Deserialize)]
struct PiJsonlEntryLite {
    #[serde(rename = "type")]
    entry_type: Option<String>,
    id: Option<String>,
}

fn validate_fork_copy(
    parent_file: &std::path::Path,
    child_file: &std::path::Path,
    requested_entry_id: &str,
) -> Result<()> {
    #[derive(Debug, serde::Deserialize)]
    struct ForkEntry {
        #[serde(rename = "type")]
        entry_type: String,
        id: String,
        #[serde(rename = "parentId")]
        parent_id: Option<String>,
        message: Option<serde_json::Value>,
    }

    fn read_entries(path: &std::path::Path) -> Result<Vec<ForkEntry>> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read fork session file {}", path.display()))?;
        let mut entries = Vec::new();
        for (line_idx, line) in raw.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let value: serde_json::Value = serde_json::from_str(line).with_context(|| {
                format!(
                    "parse fork session {} line {}",
                    path.display(),
                    line_idx + 1
                )
            })?;
            if value.get("type").and_then(serde_json::Value::as_str) == Some("session") {
                continue;
            }
            entries.push(serde_json::from_value(value).with_context(|| {
                format!("decode fork entry {} line {}", path.display(), line_idx + 1)
            })?);
        }
        Ok(entries)
    }

    let child_header: serde_json::Value = std::fs::read_to_string(child_file)
        .with_context(|| format!("read fork child header {}", child_file.display()))?
        .lines()
        .next()
        .and_then(|line| serde_json::from_str(line).ok())
        .ok_or_else(|| anyhow::anyhow!("fork child has no valid session header"))?;
    let recorded_parent = child_header
        .get("parentSession")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("fork child header has no parentSession"))?;
    let expected_parent = std::fs::canonicalize(parent_file)
        .with_context(|| format!("canonicalize fork parent {}", parent_file.display()))?;
    let recorded_parent = std::fs::canonicalize(recorded_parent)
        .with_context(|| format!("canonicalize recorded fork parent {recorded_parent}"))?;
    if recorded_parent != expected_parent {
        anyhow::bail!(
            "fork child parentSession mismatch: expected {}, got {}",
            expected_parent.display(),
            recorded_parent.display()
        );
    }

    let parent_entries = read_entries(parent_file)?;
    let parent_by_id: std::collections::HashMap<&str, &ForkEntry> = parent_entries
        .iter()
        .map(|entry| (entry.id.as_str(), entry))
        .collect();
    let requested = parent_by_id.get(requested_entry_id).ok_or_else(|| {
        anyhow::anyhow!("fork source entry not found in parent: {requested_entry_id}")
    })?;
    let requested_role = requested
        .message
        .as_ref()
        .and_then(|message| message.get("role"))
        .and_then(serde_json::Value::as_str);
    if requested.entry_type != "message" || requested_role != Some("user") {
        anyhow::bail!("fork source entry is not a user message: {requested_entry_id}");
    }

    // Pi's `fork` command uses position="before": it copies the complete path
    // ending at the selected user's parent and returns the selected prompt text
    // for the new editor. Label entries are recreated separately and therefore
    // do not belong to the copied path comparison.
    let mut expected_ids = Vec::new();
    let mut cursor = requested.parent_id.as_deref();
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = cursor {
        if !seen.insert(id.to_string()) {
            anyhow::bail!("cycle in parent fork path at entry {id}");
        }
        let entry = parent_by_id
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("fork path references missing parent entry {id}"))?;
        if entry.entry_type != "label" {
            expected_ids.push(entry.id.clone());
        }
        cursor = entry.parent_id.as_deref();
    }
    expected_ids.reverse();

    let child_entries = read_entries(child_file)?;
    let copied_entries: Vec<&ForkEntry> = child_entries
        .iter()
        .filter(|entry| entry.entry_type != "label")
        .collect();
    let actual_ids: Vec<String> = copied_entries
        .iter()
        .map(|entry| entry.id.clone())
        .collect();
    for (idx, entry) in copied_entries.iter().enumerate() {
        let expected_parent_id = idx
            .checked_sub(1)
            .map(|parent_idx| actual_ids[parent_idx].as_str());
        if entry.parent_id.as_deref() != expected_parent_id {
            anyhow::bail!(
                "fork child path is not linear at entry '{}': expected parent {:?}, got {:?}",
                entry.id,
                expected_parent_id,
                entry.parent_id
            );
        }
    }
    if actual_ids != expected_ids {
        anyhow::bail!(
            "fork copy path mismatch: expected {} entries ending at '{}', got {} ending at '{}'",
            expected_ids.len(),
            expected_ids.last().map(String::as_str).unwrap_or("<root>"),
            actual_ids.len(),
            actual_ids.last().map(String::as_str).unwrap_or("<root>")
        );
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct JsonlSessionMetadata {
    external_id: String,
    session_file: std::path::PathBuf,
    title: Option<String>,
    readable_id: Option<String>,
    workspace_path: Option<String>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

#[derive(Debug, Default)]
struct JsonlScanOutcome {
    scanned_files: usize,
    skipped_files: usize,
    failed_files: usize,
    sessions: Vec<JsonlSessionMetadata>,
}

pub(crate) fn parse_pi_session_id_from_path(path: &std::path::Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy();
    let (_, session_id) = stem.rsplit_once('_')?;
    if session_id.is_empty() {
        None
    } else {
        Some(session_id.to_string())
    }
}

fn parse_pi_created_at_ms_from_path(path: &std::path::Path) -> Option<i64> {
    let stem = path.file_stem()?.to_string_lossy();
    let (ts, _) = stem.rsplit_once('_')?;
    let parsed = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H-%M-%S-%3fZ").ok()?;
    Some(chrono::Utc.from_utc_datetime(&parsed).timestamp_millis())
}

fn parse_sqlite_datetime_ms(value: &str) -> i64 {
    chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .map(|dt| chrono::Utc.from_utc_datetime(&dt).timestamp_millis())
        .unwrap_or(0)
}

fn project_name_from_path(path: &str) -> String {
    if path == "global" || path.is_empty() {
        return "Global".to_string();
    }
    std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn select_authoritative_workspace_chat_messages(
    authoritative_messages: Option<Vec<ChatMessageProto>>,
) -> (Vec<ChatMessageProto>, &'static str) {
    if let Some(authoritative) = authoritative_messages {
        return (authoritative, "oqto-log");
    }

    (Vec::new(), "empty")
}

fn select_richest_authoritative_messages(
    projected: Option<Vec<ChatMessageProto>>,
    pi_jsonl: Option<Vec<ChatMessageProto>>,
) -> (Option<Vec<ChatMessageProto>>, &'static str) {
    match (projected, pi_jsonl) {
        // oqto-log is the authoritative history surface. Pi JSONL is used to
        // repair oqto-log before selection, not as a parallel read source with
        // synthetic IDs. Falling back to raw JSONL is only acceptable when no
        // projection exists at all.
        (Some(projected), _) => (Some(projected), "oqto-log"),
        (None, Some(pi_jsonl)) => (Some(pi_jsonl), "pi-jsonl-fallback"),
        (None, None) => (None, "empty"),
    }
}

fn select_live_workspace_chat_messages(
    live_messages: Option<Vec<ChatMessageProto>>,
) -> (Vec<ChatMessageProto>, &'static str) {
    if let Some(live) = live_messages {
        return (live, "live_buffer");
    }

    (Vec::new(), "live_buffer")
}

fn read_last_session_info_name(path: &std::path::Path) -> Option<String> {
    use std::io::BufRead;

    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    let mut last_name: Option<String> = None;

    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };

        let entry_type = value.get("type").and_then(|v| v.as_str());
        if entry_type != Some("session_info") {
            continue;
        }

        if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
            let clean = name.trim();
            if !clean.is_empty() {
                last_name = Some(clean.to_string());
            }
        }
    }

    last_name
}

async fn append_session_info_name_for_external_id(external_id: &str, name: &str) -> Result<bool> {
    let Some(path) =
        oqto_pi::session_files::find_session_file_async(external_id.to_string(), None).await
    else {
        return Ok(false);
    };
    let name = name.to_string();
    tokio::task::spawn_blocking(move || append_session_info_name(&path, &name))
        .await
        .context("join JSONL session_info append task")??;
    Ok(true)
}

fn append_session_info_name(path: &std::path::Path, name: &str) -> Result<()> {
    use std::io::{BufRead, Write};

    let clean_name = name.trim();
    if clean_name.is_empty() {
        return Ok(());
    }

    let file = std::fs::File::open(path)
        .with_context(|| format!("open session jsonl for read: {}", path.display()))?;
    let reader = std::io::BufReader::new(file);
    let mut last_id: Option<String> = None;

    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        if let Some(id) = value.get("id").and_then(|v| v.as_str()) {
            let clean = id.trim();
            if !clean.is_empty() {
                last_id = Some(clean.to_string());
            }
        }
    }

    let entry = serde_json::json!({
        "type": "session_info",
        "id": format!("si_{}", uuid::Uuid::new_v4().simple()),
        "parentId": last_id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "name": clean_name,
    });

    let mut out = std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .with_context(|| format!("open session jsonl for append: {}", path.display()))?;
    serde_json::to_writer(&mut out, &entry)
        .with_context(|| format!("write session_info entry: {}", path.display()))?;
    out.write_all(b"\n")
        .with_context(|| format!("write newline after session_info: {}", path.display()))?;
    out.flush()
        .with_context(|| format!("flush session_info append: {}", path.display()))?;

    Ok(())
}

#[derive(serde::Deserialize)]
struct JsonlMessageEntry {
    #[serde(rename = "type")]
    entry_type: String,
    id: Option<String>,
    #[serde(rename = "parentId")]
    parent_id: Option<String>,
    message: Option<oqto_pi::AgentMessage>,
}

pub(crate) fn read_jsonl_message_records(
    path: &std::path::Path,
) -> Vec<oqto_history::oqto_log::store::PiJsonlMessageRecord> {
    use std::io::BufRead;

    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(err) => {
            warn!("Failed to open Pi session file {}: {}", path.display(), err);
            return Vec::new();
        }
    };

    let reader = std::io::BufReader::new(file);
    let mut records = Vec::new();

    for (line_idx, line) in reader.lines().map_while(Result::ok).enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let entry: JsonlMessageEntry = match serde_json::from_str(trimmed) {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        if entry.entry_type != "message" {
            continue;
        }

        if let Some(message) = entry.message {
            records.push(oqto_history::oqto_log::store::PiJsonlMessageRecord {
                source_entry_id: entry.id.unwrap_or_else(|| format!("line:{line_idx}")),
                parent_source_entry_id: entry.parent_id,
                source_sequence: line_idx as i64,
                message,
            });
        }
    }

    records
}

fn read_jsonl_agent_messages(path: &std::path::Path) -> Vec<oqto_pi::AgentMessage> {
    read_jsonl_message_records(path)
        .into_iter()
        .map(|record| record.message)
        .collect()
}

fn scan_pi_jsonl_session_metadata(limit: Option<usize>) -> JsonlScanOutcome {
    let mut outcome = JsonlScanOutcome::default();

    let Ok(home) = std::env::var("HOME") else {
        return outcome;
    };
    let base = std::path::PathBuf::from(home).join(".pi/agent/sessions");
    let Ok(workspaces) = std::fs::read_dir(base) else {
        return outcome;
    };

    let mut files: Vec<std::path::PathBuf> = Vec::new();
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
            if path.is_file() && path.extension().and_then(|v| v.to_str()) == Some("jsonl") {
                files.push(path);
            }
        }
    }

    files.sort();
    files.reverse();

    if let Some(limit) = limit
        && files.len() > limit
    {
        files.truncate(limit);
    }

    for path in files {
        outcome.scanned_files += 1;

        let Some(external_id) = parse_pi_session_id_from_path(&path) else {
            outcome.skipped_files += 1;
            continue;
        };

        // The safe dirname encoding is lossy for paths containing '-';
        // the JSONL header cwd is the exact workspace.
        let workspace_path = super::jsonl_ingest::read_session_cwd(&path);

        let metadata = std::fs::metadata(&path);
        let modified_ms = metadata
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let created_at_ms = parse_pi_created_at_ms_from_path(&path).unwrap_or(modified_ms);
        let updated_at_ms = if modified_ms > 0 {
            modified_ms
        } else {
            created_at_ms
        };

        let session_name = read_last_session_info_name(&path);
        let (title, readable_id) = if let Some(name) = session_name {
            let parsed = oqto_pi::session_parser::ParsedTitle::parse(&name);
            let readable_id = parsed.get_readable_id().map(ToOwned::to_owned);
            let parsed_title = parsed.display_title().trim();
            let fallback_title = name
                .split('[')
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let title = if !parsed_title.is_empty() {
                Some(parsed_title.to_string())
            } else {
                fallback_title
            };
            (title, readable_id)
        } else {
            (None, None)
        };

        outcome.sessions.push(JsonlSessionMetadata {
            external_id,
            session_file: path,
            title,
            readable_id,
            workspace_path,
            created_at_ms,
            updated_at_ms,
        });
    }

    outcome
}

impl Runner {
    pub fn new(
        sandbox_config: Option<SandboxConfig>,
        binaries: SessionBinaries,
        user_config: RunnerUserConfig,
        pi_manager: Arc<PiSessionManager>,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self {
            state: Arc::new(RwLock::new(RunnerState::new())),
            shutdown_tx,
            sandbox_config,
            binaries,
            user_config,
            pi_manager,
            jsonl_ingest: super::jsonl_ingest::spawn(),
        }
    }

    fn request_kind(req: &RunnerRequest) -> String {
        serde_json::to_value(req)
            .ok()
            .and_then(|v| {
                v.get("type")
                    .and_then(|t| t.as_str())
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn request_timeout(req: &RunnerRequest) -> std::time::Duration {
        match req {
            // Long-lived stream: handled separately in connection loop.
            RunnerRequest::PiSubscribe(_) | RunnerRequest::SubscribeStdout(_) => {
                std::time::Duration::from_secs(300)
            }
            // These can legitimately take longer due process startup/teardown.
            RunnerRequest::PiCreateSession(_)
            | RunnerRequest::PiDeleteSession(_)
            | RunnerRequest::PiCloseSession(_) => std::time::Duration::from_secs(20),
            // Scanning and repairing JSONL chat metadata can be expensive for
            // users with large session histories.
            RunnerRequest::RepairWorkspaceChatHistory(_) => std::time::Duration::from_secs(120),
            _ => std::time::Duration::from_secs(10),
        }
    }

    /// Handle a single request.
    async fn handle_request(&self, req: RunnerRequest) -> RunnerResponse {
        handlers::dispatch::handle_request(self, req).await
    }

    /// Get stdout broadcast receiver for a process.
    async fn get_stdout_receiver(
        &self,
        process_id: &str,
    ) -> Result<(broadcast::Receiver<StdoutEvent>, Vec<String>), RunnerResponse> {
        let state = self.state.read().await;

        let Some(proc) = state.processes.get(process_id) else {
            return Err(error_response(
                ErrorCode::ProcessNotFound,
                format!("Process '{}' not found", process_id),
            ));
        };

        if !proc.is_rpc {
            return Err(error_response(
                ErrorCode::NotRpcProcess,
                format!("Process '{}' is not an RPC process", process_id),
            ));
        }

        let Some(ref tx) = proc.stdout_tx else {
            return Err(error_response(
                ErrorCode::IoError,
                "stdout channel not available",
            ));
        };

        // Get any buffered lines first
        let buffered_lines = if let Some(ref buffer) = proc.stdout_buffer {
            let buf = buffer.lock().await;
            buf.lines.clone()
        } else {
            Vec::new()
        };

        Ok((tx.subscribe(), buffered_lines))
    }

    async fn spawn_process(&self, req: SpawnProcessRequest, is_rpc: bool) -> RunnerResponse {
        let mut state = self.state.write().await;

        // Check if ID already exists
        if state.processes.contains_key(&req.id) {
            return error_response(
                ErrorCode::ProcessAlreadyExists,
                format!("Process with ID '{}' already exists", req.id),
            );
        }

        // Determine if we should sandbox this process
        let use_sandbox = req.sandboxed && self.sandbox_config.is_some();

        // SECURITY: If sandbox is requested but not available, refuse to run
        // This prevents accidental unsandboxed execution when sandbox is expected
        if req.sandboxed && self.sandbox_config.is_none() {
            error!(
                "SECURITY: Sandbox requested for '{}' but no sandbox config loaded. \
                 Refusing to run unsandboxed. Load sandbox config from /etc/oqto/sandbox.toml \
                 or pass --sandbox-config to oqto-runner.",
                req.id
            );
            return error_response(
                ErrorCode::SandboxError,
                format!(
                    "Sandbox requested but no sandbox config loaded. \
                     Cannot run '{}' without sandbox configuration.",
                    req.binary
                ),
            );
        }

        // Build command - either direct or via oqto-sandbox
        let mut seccomp_file_for_spawn: Option<std::fs::File> = None;
        let mut set_no_new_privs = false;
        let mut landlock_cfg_for_spawn: Option<SandboxConfig> = None;
        let mut landlock_workspace_for_spawn: Option<PathBuf> = None;

        let (program, args, effective_binary) = if use_sandbox {
            let Some(sandbox_config) = self.sandbox_config.as_ref() else {
                return error_response(
                    ErrorCode::SandboxError,
                    "Sandbox requested but no sandbox config loaded".to_string(),
                );
            };

            // Build bwrap args using the trusted config
            // Note: We use the current user (runner's user) for path expansion
            match sandbox_config.build_bwrap_args_for_user(&req.cwd, None) {
                Some(bwrap_args) => {
                    // Command: bwrap [bwrap_args] -- binary [args]
                    let mut full_args = bwrap_args;
                    full_args.push(req.binary.clone());
                    full_args.extend(req.args.iter().cloned());

                    match sandbox_config.open_seccomp_bpf_file(None) {
                        Ok(file) => {
                            seccomp_file_for_spawn = file;
                        }
                        Err(e) => {
                            return error_response(
                                ErrorCode::SandboxError,
                                format!("Failed to prepare seccomp policy: {}", e),
                            );
                        }
                    }
                    set_no_new_privs = sandbox_config.no_new_privs;
                    landlock_cfg_for_spawn = Some(sandbox_config.clone());
                    landlock_workspace_for_spawn = Some(req.cwd.clone());

                    info!(
                        "Sandboxing process '{}' with {} bwrap args",
                        req.id,
                        full_args.len()
                    );
                    debug!("bwrap command: bwrap {}", full_args.join(" "));

                    ("bwrap".to_string(), full_args, req.binary.clone())
                }
                None => {
                    // SECURITY: bwrap not available - refuse to run
                    error!(
                        "SECURITY: Sandbox requested for '{}' but bwrap not available. \
                         Install bubblewrap (bwrap) or disable sandboxing.",
                        req.id
                    );
                    return error_response(
                        ErrorCode::SandboxError,
                        format!(
                            "Sandbox requested but bwrap not available. \
                             Cannot run '{}' without bubblewrap installed.",
                            req.binary
                        ),
                    );
                }
            }
        } else {
            (req.binary.clone(), req.args.clone(), req.binary.clone())
        };

        // Build the command
        let mut cmd = Command::new(&program);
        cmd.args(&args);
        // Note: For sandboxed processes, cwd is handled by bwrap's workspace bind
        // For non-sandboxed, we set it directly
        if !use_sandbox {
            cmd.current_dir(&req.cwd);
        }
        cmd.envs(&req.env);

        #[cfg(target_os = "linux")]
        {
            let seccomp_file = seccomp_file_for_spawn;
            let seccomp_fd = seccomp_file.as_ref().map(AsRawFd::as_raw_fd);
            let landlock_cfg = landlock_cfg_for_spawn;
            let landlock_workspace = landlock_workspace_for_spawn;
            if seccomp_fd.is_some() || set_no_new_privs || landlock_cfg.is_some() {
                // Keep file alive until spawn by moving into closure.
                let _keep_alive = seccomp_file;
                // SAFETY: pre_exec runs in child process after fork and before exec.
                unsafe {
                    cmd.pre_exec(move || {
                        if set_no_new_privs {
                            let rc = libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
                            if rc != 0 {
                                return Err(std::io::Error::last_os_error());
                            }
                        }

                        if let (Some(cfg), Some(workspace)) =
                            (landlock_cfg.as_ref(), landlock_workspace.as_ref())
                        {
                            cfg.apply_landlock(workspace, None)?;
                        }

                        if let Some(fd) = seccomp_fd
                            && libc::dup2(fd, 3) == -1
                        {
                            return Err(std::io::Error::last_os_error());
                        }
                        Ok(())
                    });
                }
            }
        }

        if is_rpc {
            cmd.stdin(Stdio::piped());
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
        } else {
            cmd.stdin(Stdio::null());
            cmd.stdout(Stdio::null());
            cmd.stderr(Stdio::null());
        }

        // Spawn
        match cmd.spawn() {
            Ok(mut child) => {
                let pid = child.id().unwrap_or(0);
                info!(
                    "Spawned process '{}': {} {:?} (pid={}, rpc={}, sandboxed={})",
                    req.id, effective_binary, req.args, pid, is_rpc, use_sandbox
                );

                // For RPC processes, set up background stdout reader
                let (stdout_buffer, stdout_tx, reader_handle) = if is_rpc {
                    let buffer = Arc::new(Mutex::new(StdoutBuffer::new()));
                    let (tx, _) = broadcast::channel::<StdoutEvent>(256);

                    // Take stdout from the child
                    let stdout = child.stdout.take();
                    let stderr = child.stderr.take();

                    // Spawn background task to read stdout
                    let buffer_clone = Arc::clone(&buffer);
                    let tx_clone = tx.clone();
                    let process_id = req.id.clone();
                    let handle = tokio::spawn(async move {
                        Self::stdout_reader_task(
                            process_id,
                            stdout,
                            stderr,
                            buffer_clone,
                            tx_clone,
                        )
                        .await;
                    });

                    (Some(buffer), Some(tx), Some(handle))
                } else {
                    (None, None, None)
                };

                let managed = ManagedProcess {
                    id: req.id.clone(),
                    pid,
                    binary: effective_binary,
                    cwd: req.cwd,
                    child,
                    is_rpc,
                    stdout_buffer,
                    stdout_tx,
                    _reader_handle: reader_handle,
                };

                state.processes.insert(req.id.clone(), managed);

                RunnerResponse::ProcessSpawned(ProcessSpawnedResponse { id: req.id, pid })
            }
            Err(e) => {
                error!("Failed to spawn process '{}': {}", req.id, e);
                error_response(ErrorCode::SpawnFailed, e.to_string())
            }
        }
    }

    /// Background task that reads stdout/stderr and buffers the lines.
    async fn stdout_reader_task(
        process_id: String,
        stdout: Option<tokio::process::ChildStdout>,
        stderr: Option<tokio::process::ChildStderr>,
        buffer: Arc<Mutex<StdoutBuffer>>,
        stdout_tx: broadcast::Sender<StdoutEvent>,
    ) {
        // Read both stdout and stderr concurrently
        let buffer_clone = Arc::clone(&buffer);
        let stdout_tx_clone = stdout_tx.clone();
        let stdout_task = async move {
            if let Some(stdout) = stdout {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    // Buffer the line
                    {
                        let mut buf = buffer_clone.lock().await;
                        buf.lines.push(line.clone());
                        // Keep buffer size reasonable (max 10000 lines)
                        if buf.lines.len() > 10000 {
                            buf.lines.remove(0);
                        }
                    }
                    // Broadcast to subscribers (ignore errors if no subscribers)
                    let _ = stdout_tx_clone.send(StdoutEvent::Line(line));
                }
            }
        };

        let stderr_task = async {
            if let Some(stderr) = stderr {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    // Log stderr but don't buffer it (it's for debugging)
                    if !line.trim().is_empty() {
                        debug!("Process '{}' stderr: {}", process_id, line);
                    }
                }
            }
        };

        // Run both tasks concurrently
        tokio::join!(stdout_task, stderr_task);

        // Mark buffer as closed when process exits
        let mut buf = buffer.lock().await;
        buf.closed = true;
        info!("Stdout reader for process '{}' finished", process_id);

        // Notify subscribers that stdout ended
        let _ = stdout_tx.send(StdoutEvent::Closed {
            exit_code: buf.exit_code,
        });
    }

    async fn kill_process(&self, req: KillProcessRequest) -> RunnerResponse {
        let mut state = self.state.write().await;

        let Some(proc) = state.processes.get_mut(&req.id) else {
            return error_response(
                ErrorCode::ProcessNotFound,
                format!("Process '{}' not found", req.id),
            );
        };

        let was_running = proc.is_running();

        if was_running {
            let result = if req.force {
                proc.child.kill().await
            } else {
                // Send SIGTERM via start_kill (doesn't wait)
                proc.child.start_kill()
            };

            if let Err(e) = result {
                warn!("Error killing process '{}': {}", req.id, e);
            }
        }

        // Remove from tracking (this will drop the reader handle, cancelling the task)
        state.processes.remove(&req.id);

        info!("Killed process '{}' (was_running={})", req.id, was_running);

        RunnerResponse::ProcessKilled(ProcessKilledResponse {
            id: req.id,
            was_running,
        })
    }

    async fn get_status(&self, req: GetStatusRequest) -> RunnerResponse {
        let mut state = self.state.write().await;

        let Some(proc) = state.processes.get_mut(&req.id) else {
            return error_response(
                ErrorCode::ProcessNotFound,
                format!("Process '{}' not found", req.id),
            );
        };

        let running = proc.is_running();
        let exit_code = proc.exit_code();

        RunnerResponse::ProcessStatus(ProcessStatusResponse {
            id: req.id,
            running,
            pid: Some(proc.pid),
            exit_code,
        })
    }

    async fn list_processes(&self) -> RunnerResponse {
        let mut state = self.state.write().await;

        let processes: Vec<ProcessInfo> = state
            .processes
            .values_mut()
            .map(|p| ProcessInfo {
                id: p.id.clone(),
                pid: p.pid,
                binary: p.binary.clone(),
                cwd: p.cwd.clone(),
                is_rpc: p.is_rpc,
                running: p.is_running(),
            })
            .collect();

        RunnerResponse::ProcessList(ProcessListResponse { processes })
    }

    async fn write_stdin(&self, req: WriteStdinRequest) -> RunnerResponse {
        let mut state = self.state.write().await;

        let Some(proc) = state.processes.get_mut(&req.id) else {
            return error_response(
                ErrorCode::ProcessNotFound,
                format!("Process '{}' not found", req.id),
            );
        };

        if !proc.is_rpc {
            return error_response(
                ErrorCode::NotRpcProcess,
                format!("Process '{}' is not an RPC process", req.id),
            );
        }

        let Some(stdin) = proc.child.stdin.as_mut() else {
            return error_response(ErrorCode::IoError, "stdin not available");
        };

        match stdin.write_all(req.data.as_bytes()).await {
            Ok(()) => {
                // Flush to ensure data is sent immediately
                if let Err(e) = stdin.flush().await {
                    return error_response(ErrorCode::IoError, format!("flush failed: {}", e));
                }
                let bytes_written = req.data.len();
                debug!("Wrote {} bytes to stdin of '{}'", bytes_written, req.id);
                RunnerResponse::StdinWritten(StdinWrittenResponse {
                    id: req.id,
                    bytes_written,
                })
            }
            Err(e) => error_response(ErrorCode::IoError, e.to_string()),
        }
    }

    async fn read_stdout(&self, req: ReadStdoutRequest) -> RunnerResponse {
        // Get the buffer reference without holding the state lock
        let buffer = {
            let state = self.state.read().await;

            let Some(proc) = state.processes.get(&req.id) else {
                return error_response(
                    ErrorCode::ProcessNotFound,
                    format!("Process '{}' not found", req.id),
                );
            };

            if !proc.is_rpc {
                return error_response(
                    ErrorCode::NotRpcProcess,
                    format!("Process '{}' is not an RPC process", req.id),
                );
            }

            let Some(ref buffer) = proc.stdout_buffer else {
                return error_response(ErrorCode::IoError, "stdout buffer not available");
            };

            Arc::clone(buffer)
        };

        // If timeout is specified, wait for data
        if req.timeout_ms > 0 {
            let timeout = std::time::Duration::from_millis(req.timeout_ms);
            let start = std::time::Instant::now();

            while start.elapsed() < timeout {
                let buf = buffer.lock().await;
                if !buf.lines.is_empty() || buf.closed {
                    break;
                }
                drop(buf);
                // Small sleep to avoid busy loop
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        }

        // Get buffered data
        let mut buf = buffer.lock().await;
        if buf.lines.is_empty() {
            return RunnerResponse::StdoutRead(StdoutReadResponse {
                id: req.id,
                data: String::new(),
                has_more: !buf.closed,
            });
        }

        // Return all buffered lines joined with newlines
        let data = buf.lines.join("\n") + "\n";
        let has_more = !buf.closed;
        buf.lines.clear();

        RunnerResponse::StdoutRead(StdoutReadResponse {
            id: req.id,
            data,
            has_more,
        })
    }

    // ========================================================================
    // Filesystem Operations (user-plane)
    // ========================================================================

    async fn read_file(&self, req: ReadFileRequest) -> RunnerResponse {
        use base64::Engine;

        let path = &req.path;

        // Validate path is within allowed workspace
        // For now, allow any path the runner's user can access
        // TODO: Add workspace root validation

        match tokio::fs::read(path).await {
            Ok(content) => {
                let size = content.len() as u64;
                let (data, truncated) = if let Some(limit) = req.limit {
                    let offset = req.offset.unwrap_or(0) as usize;
                    let end = (offset + limit as usize).min(content.len());
                    let slice = &content[offset.min(content.len())..end];
                    (slice.to_vec(), end < content.len())
                } else {
                    (content, false)
                };

                let content_base64 = base64::engine::general_purpose::STANDARD.encode(&data);

                RunnerResponse::FileContent(FileContentResponse {
                    path: path.clone(),
                    content_base64,
                    size,
                    truncated,
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => error_response(
                ErrorCode::PathNotFound,
                format!("File not found: {:?}", path),
            ),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => error_response(
                ErrorCode::PermissionDenied,
                format!("Permission denied: {:?}", path),
            ),
            Err(e) => error_response(ErrorCode::IoError, format!("Read error: {}", e)),
        }
    }

    async fn write_file(&self, req: WriteFileRequest) -> RunnerResponse {
        use base64::Engine;

        let path = &req.path;

        // Decode base64 content
        let content = match base64::engine::general_purpose::STANDARD.decode(&req.content_base64) {
            Ok(c) => c,
            Err(e) => {
                return error_response(
                    ErrorCode::InvalidRequest,
                    format!("Invalid base64 content: {}", e),
                );
            }
        };

        // Create parent directories if requested
        if req.create_parents
            && let Some(parent) = path.parent()
            && let Err(e) = tokio::fs::create_dir_all(parent).await
        {
            return error_response(
                ErrorCode::IoError,
                format!("Failed to create parent directories: {}", e),
            );
        }

        match tokio::fs::write(path, &content).await {
            Ok(()) => RunnerResponse::FileWritten(FileWrittenResponse {
                path: path.clone(),
                bytes_written: content.len() as u64,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => error_response(
                ErrorCode::PermissionDenied,
                format!("Permission denied: {:?}", path),
            ),
            Err(e) => error_response(ErrorCode::IoError, format!("Write error: {}", e)),
        }
    }

    async fn list_directory(&self, req: ListDirectoryRequest) -> RunnerResponse {
        let path = &req.path;

        let mut entries = Vec::new();

        let mut dir = match tokio::fs::read_dir(path).await {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return error_response(
                    ErrorCode::PathNotFound,
                    format!("Directory not found: {:?}", path),
                );
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                return error_response(
                    ErrorCode::PermissionDenied,
                    format!("Permission denied: {:?}", path),
                );
            }
            Err(e) => {
                return error_response(ErrorCode::IoError, format!("Read dir error: {}", e));
            }
        };

        while let Ok(Some(entry)) = dir.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files unless requested
            if !req.include_hidden && name.starts_with('.') {
                continue;
            }

            let metadata = match entry.metadata().await {
                Ok(m) => m,
                Err(_) => continue,
            };

            let modified_at = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            entries.push(DirEntry {
                name,
                is_dir: metadata.is_dir(),
                is_symlink: metadata.is_symlink(),
                size: metadata.len(),
                modified_at,
            });
        }

        // Sort by name
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        RunnerResponse::DirectoryListing(DirectoryListingResponse {
            path: path.clone(),
            entries,
        })
    }

    async fn stat(&self, req: StatRequest) -> RunnerResponse {
        let path = &req.path;

        match tokio::fs::metadata(path).await {
            Ok(metadata) => {
                let modified_at = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);

                let created_at = metadata
                    .created()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64);

                #[cfg(unix)]
                let mode = {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode()
                };
                #[cfg(not(unix))]
                let mode = 0o644;

                RunnerResponse::FileStat(FileStatResponse {
                    path: path.clone(),
                    exists: true,
                    is_file: metadata.is_file(),
                    is_dir: metadata.is_dir(),
                    is_symlink: metadata.is_symlink(),
                    size: metadata.len(),
                    modified_at,
                    created_at,
                    mode,
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                RunnerResponse::FileStat(FileStatResponse {
                    path: path.clone(),
                    exists: false,
                    is_file: false,
                    is_dir: false,
                    is_symlink: false,
                    size: 0,
                    modified_at: 0,
                    created_at: None,
                    mode: 0,
                })
            }
            Err(e) => error_response(ErrorCode::IoError, format!("Stat error: {}", e)),
        }
    }

    async fn delete_path(&self, req: DeletePathRequest) -> RunnerResponse {
        let path = &req.path;

        // Check if path exists and what type it is
        let metadata = match tokio::fs::metadata(path).await {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return error_response(
                    ErrorCode::PathNotFound,
                    format!("Path not found: {:?}", path),
                );
            }
            Err(e) => {
                return error_response(ErrorCode::IoError, format!("Metadata error: {}", e));
            }
        };

        let result = if metadata.is_dir() {
            if req.recursive {
                tokio::fs::remove_dir_all(path).await
            } else {
                tokio::fs::remove_dir(path).await
            }
        } else {
            tokio::fs::remove_file(path).await
        };

        match result {
            Ok(()) => RunnerResponse::PathDeleted(PathDeletedResponse { path: path.clone() }),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => error_response(
                ErrorCode::PermissionDenied,
                format!("Permission denied: {:?}", path),
            ),
            Err(e) => error_response(ErrorCode::IoError, format!("Delete error: {}", e)),
        }
    }

    async fn create_directory(&self, req: CreateDirectoryRequest) -> RunnerResponse {
        let path = &req.path;

        let result = if req.create_parents {
            tokio::fs::create_dir_all(path).await
        } else {
            tokio::fs::create_dir(path).await
        };

        match result {
            Ok(()) => {
                RunnerResponse::DirectoryCreated(DirectoryCreatedResponse { path: path.clone() })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => error_response(
                ErrorCode::PathExists,
                format!("Path already exists: {:?}", path),
            ),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => error_response(
                ErrorCode::PermissionDenied,
                format!("Permission denied: {:?}", path),
            ),
            Err(e) => error_response(ErrorCode::IoError, format!("Create dir error: {}", e)),
        }
    }

    // ========================================================================
    // Session Operations (user-plane)
    // ========================================================================

    async fn list_sessions(&self) -> RunnerResponse {
        let state = self.state.read().await;
        let sessions: Vec<SessionInfo> = state
            .sessions
            .values()
            .map(|s| {
                // Check if processes are still running
                let status = "running".to_string(); // We track active sessions only
                SessionInfo {
                    id: s.id.clone(),
                    workspace_path: s.workspace_path.clone(),
                    status,
                    agent_port: None,
                    fileserver_port: Some(s.fileserver_port),
                    ttyd_port: Some(s.ttyd_port),
                    pids: Some(format!("{},{}", s.fileserver_id, s.ttyd_id)),
                    created_at: chrono::Utc::now().to_rfc3339(), // TODO: track actual time
                    started_at: Some(chrono::Utc::now().to_rfc3339()),
                    last_activity_at: None,
                }
            })
            .collect();

        RunnerResponse::SessionList(SessionListResponse { sessions })
    }

    async fn get_session(&self, req: GetSessionRequest) -> RunnerResponse {
        let state = self.state.read().await;
        let session = state.sessions.get(&req.session_id).map(|s| SessionInfo {
            id: s.id.clone(),
            workspace_path: s.workspace_path.clone(),
            status: "running".to_string(),
            agent_port: None,
            fileserver_port: Some(s.fileserver_port),
            ttyd_port: Some(s.ttyd_port),
            pids: Some(format!("{},{}", s.fileserver_id, s.ttyd_id)),
            created_at: chrono::Utc::now().to_rfc3339(),
            started_at: Some(chrono::Utc::now().to_rfc3339()),
            last_activity_at: None,
        });

        RunnerResponse::Session(SessionResponse { session })
    }

    async fn get_terminal_credential(&self, req: GetTerminalCredentialRequest) -> RunnerResponse {
        let state = self.state.read().await;
        match state.sessions.get(&req.session_id) {
            Some(session) => RunnerResponse::TerminalCredential(TerminalCredentialResponse {
                session_id: req.session_id.clone(),
                username: TERMINAL_USERNAME.to_string(),
                password: session.ttyd_credential.clone(),
            }),
            None => error_response(
                ErrorCode::SessionNotFound,
                format!("Session {} not found", req.session_id),
            ),
        }
    }

    async fn start_session(&self, req: StartSessionRequest) -> RunnerResponse {
        info!(
            "Starting session {} in {:?} with ports fs={}/ttyd={}",
            req.session_id, req.workspace_path, req.fileserver_port, req.ttyd_port
        );

        // Check if session already exists
        {
            let state = self.state.read().await;
            if state.sessions.contains_key(&req.session_id) {
                return error_response(
                    ErrorCode::SessionExists,
                    format!("Session {} already exists", req.session_id),
                );
            }
        }

        // Ensure workspace directory exists
        if let Err(e) = tokio::fs::create_dir_all(&req.workspace_path).await {
            return error_response(
                ErrorCode::IoError,
                format!("Failed to create workspace directory: {}", e),
            );
        }

        // Generate unique process IDs for this session
        let fileserver_id = format!("{}-fileserver", req.session_id);
        let ttyd_id = format!("{}-ttyd", req.session_id);

        // Spawn fileserver. Its upload limit is independent from Axum's
        // control-plane limit, so pass the deployment-wide files config when
        // present to keep both enforcement points aligned.
        let mut fileserver_args = vec![
            "--port".to_string(),
            req.fileserver_port.to_string(),
            "--bind".to_string(),
            "127.0.0.1".to_string(),
            "--root".to_string(),
            req.workspace_path.to_string_lossy().to_string(),
        ];
        let files_config = Path::new("/etc/oqto/files.toml");
        if files_config.is_file() {
            fileserver_args.extend([
                "--config".to_string(),
                files_config.to_string_lossy().into_owned(),
            ]);
        }
        let fileserver_req = SpawnProcessRequest {
            id: fileserver_id.clone(),
            binary: self.binaries.fileserver.clone(),
            args: fileserver_args,
            cwd: req.workspace_path.clone(),
            env: HashMap::new(),
            sandboxed: false,
        };

        if let RunnerResponse::Error(e) = self.spawn_process(fileserver_req, false).await {
            return RunnerResponse::Error(e);
        }

        let ttyd_credential = if self.user_config.terminal_enabled {
            Some(generate_terminal_credential())
        } else {
            info!(
                "Terminal disabled by policy; not spawning ttyd for session {}",
                req.session_id
            );
            None
        };

        if let Some(ref password) = ttyd_credential {
            let ttyd_req = SpawnProcessRequest {
                id: ttyd_id.clone(),
                binary: self.binaries.ttyd.clone(),
                args: build_ttyd_args(
                    req.ttyd_port,
                    &req.workspace_path.to_string_lossy(),
                    password,
                ),
                cwd: req.workspace_path.clone(),
                env: HashMap::new(),
                sandboxed: false,
            };

            if let RunnerResponse::Error(e) = self.spawn_process(ttyd_req, false).await {
                // Clean up fileserver
                let _ = self
                    .kill_process(KillProcessRequest {
                        id: fileserver_id.clone(),
                        force: false,
                    })
                    .await;
                return RunnerResponse::Error(e);
            }
        }

        // Record session state (Pi agent is managed separately by PiSessionManager)
        let session_state = SessionState {
            id: req.session_id.clone(),
            workspace_path: req.workspace_path.clone(),
            fileserver_id: fileserver_id.clone(),
            ttyd_id: ttyd_id.clone(),
            fileserver_port: req.fileserver_port,
            ttyd_port: if ttyd_credential.is_some() {
                req.ttyd_port
            } else {
                0
            },
            ttyd_credential,
            agent: req.agent.clone(),
            started_at: std::time::Instant::now(),
        };

        {
            let mut state = self.state.write().await;
            state.sessions.insert(req.session_id.clone(), session_state);
        }

        let pids = format!("{},{}", fileserver_id, ttyd_id);
        info!(
            "Session {} started with processes: {}",
            req.session_id, pids
        );

        RunnerResponse::SessionStarted(SessionStartedResponse {
            session_id: req.session_id,
            pids,
        })
    }

    async fn stop_session(&self, req: StopSessionRequest) -> RunnerResponse {
        info!("Stopping session {}", req.session_id);

        let session_state = {
            let mut state = self.state.write().await;
            state.sessions.remove(&req.session_id)
        };

        let session_state = match session_state {
            Some(s) => s,
            None => {
                return error_response(
                    ErrorCode::SessionNotFound,
                    format!("Session {} not found", req.session_id),
                );
            }
        };

        // Kill session processes (fileserver + ttyd)
        let _ = self
            .kill_process(KillProcessRequest {
                id: session_state.fileserver_id,
                force: false,
            })
            .await;

        let _ = self
            .kill_process(KillProcessRequest {
                id: session_state.ttyd_id,
                force: false,
            })
            .await;

        info!("Session {} stopped", req.session_id);

        RunnerResponse::SessionStopped(SessionStoppedResponse {
            session_id: req.session_id,
        })
    }

    // ========================================================================
    // Main Chat Operations (user-plane)
    // ========================================================================

    async fn list_main_chat_sessions(&self) -> RunnerResponse {
        RunnerResponse::MainChatSessionList(MainChatSessionListResponse {
            sessions: Vec::new(),
        })
    }
    async fn get_main_chat_messages(&self, req: GetMainChatMessagesRequest) -> RunnerResponse {
        RunnerResponse::MainChatMessages(MainChatMessagesResponse {
            session_id: req.session_id,
            messages: Vec::new(),
        })
    }
    async fn get_workspace_chat_messages(
        &self,
        req: GetWorkspaceChatMessagesRequest,
    ) -> RunnerResponse {
        RunnerResponse::WorkspaceChatMessages(MainChatMessagesResponse {
            session_id: req.session_id,
            messages: Vec::new(),
        })
    }
    async fn list_workspace_chat_sessions(
        &self,
        req: ListWorkspaceChatSessionsRequest,
    ) -> RunnerResponse {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let rows = match oqto_history::oqto_log::ops::list_sessions(
            std::path::Path::new(&home),
            req.workspace.as_deref(),
        )
        .await
        {
            Ok(rows) => rows,
            Err(e) => {
                return error_response(
                    ErrorCode::IoError,
                    format!("Failed to query oqto-log sessions: {e}"),
                );
            }
        };

        let mut sessions = Vec::with_capacity(rows.len());
        for row in rows {
            let workspace_path = row.workspace_id.unwrap_or_else(|| "global".to_string());
            if self.user_config.linux_users_enabled
                && !self.user_config.single_user
                && workspace_path != "global"
                && !workspace_path.starts_with(&home)
            {
                tracing::warn!(
                    workspace = %workspace_path,
                    home = %home,
                    "Filtering out session with foreign workspace path"
                );
                continue;
            }

            // Keep session listing hot: never scan Pi JSONL files here.
            // Titles/readable ids are synced into oqto-log by explicit/background
            // sync paths; blocking the sidebar on per-session file scans makes
            // every workdir show 0 sessions until the full scan completes.
            let title = row.title;
            let readable_id = row.readable_id.unwrap_or_default();
            let project_name = project_name_from_path(&workspace_path);
            let parent_id = row.parent_session_id.clone();
            sessions.push(WorkspaceChatSessionInfo {
                id: if row.platform_id.is_empty() {
                    row.session_id
                } else {
                    row.platform_id
                },
                readable_id,
                title,
                parent_id,
                workspace_path,
                project_name,
                created_at: parse_sqlite_datetime_ms(&row.created_at),
                updated_at: parse_sqlite_datetime_ms(&row.updated_at),
                version: Some(row.messages.max(0).to_string()),
                is_child: row.parent_session_id.is_some(),
                model: None,
                provider: None,
            });
        }

        if let Some(limit) = req.limit {
            sessions.truncate(limit);
        }

        RunnerResponse::WorkspaceChatSessionList(WorkspaceChatSessionListResponse { sessions })
    }

    async fn get_workspace_chat_session(
        &self,
        req: GetWorkspaceChatSessionRequest,
    ) -> RunnerResponse {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let row = match oqto_history::oqto_log::ops::get_session(
            std::path::Path::new(&home),
            &req.session_id,
        )
        .await
        {
            Ok(Some(row)) => row,
            Ok(None) => {
                return RunnerResponse::WorkspaceChatSession(WorkspaceChatSessionResponse {
                    session: None,
                });
            }
            Err(e) => {
                return error_response(
                    ErrorCode::IoError,
                    format!("Failed to load oqto-log session: {e}"),
                );
            }
        };

        let workspace_path = row.workspace_id.unwrap_or_else(|| "global".to_string());
        // oqto-log is the read authority; never scan Pi JSONL on this hot path.
        let parsed_title = row
            .title
            .as_deref()
            .map(oqto_pi::session_parser::ParsedTitle::parse);
        let title = parsed_title
            .as_ref()
            .map(|parsed| parsed.display_title().to_string())
            .filter(|title| !title.is_empty());
        let readable_id = row
            .readable_id
            .clone()
            .or_else(|| parsed_title.and_then(|parsed| parsed.readable_id))
            .unwrap_or_default();
        let project_name = project_name_from_path(&workspace_path);
        let parent_id = row.parent_session_id.clone();
        let session = WorkspaceChatSessionInfo {
            id: if row.platform_id.is_empty() {
                row.session_id
            } else {
                row.platform_id
            },
            readable_id,
            title,
            parent_id,
            workspace_path,
            project_name,
            created_at: parse_sqlite_datetime_ms(&row.created_at),
            updated_at: parse_sqlite_datetime_ms(&row.updated_at),
            version: Some(row.messages.max(0).to_string()),
            is_child: row.parent_session_id.is_some(),
            model: None,
            provider: None,
        };

        RunnerResponse::WorkspaceChatSession(WorkspaceChatSessionResponse {
            session: Some(session),
        })
    }

    async fn get_workspace_chat_session_messages(
        &self,
        req: GetWorkspaceChatSessionMessagesRequest,
    ) -> RunnerResponse {
        let session_is_active = self.pi_manager.has_session(&req.session_id).await;

        let (messages, source_label, source_mode) = match req.source {
            WorkspaceChatMessagesSource::Authoritative => {
                let authoritative_messages = if let Ok(home) = std::env::var("HOME") {
                    let home_path = std::path::Path::new(&home);
                    let projected =
                        match oqto_history::oqto_log::projector::project_session_messages_auto(
                            home_path,
                            &req.session_id,
                            req.limit,
                        )
                        .await
                        {
                            Ok(messages) => messages,
                            Err(err) => {
                                debug!(
                                    "get_workspace_chat_session_messages session={} source=oqto-log error={}",
                                    req.session_id, err
                                );
                                None
                            }
                        };

                    if req.limit.is_none() {
                        // Reads are read-only: nudge the serialized background
                        // ingest task to stat this session's JSONL for
                        // out-of-band (bare pi) changes.
                        self.jsonl_ingest.nudge(&req.session_id);
                        projected.filter(|messages| !messages.is_empty())
                    } else {
                        projected
                    }
                } else {
                    None
                };

                let (messages, source) =
                    select_authoritative_workspace_chat_messages(authoritative_messages);
                (messages, source, WorkspaceChatMessagesSource::Authoritative)
            }
            WorkspaceChatMessagesSource::Live => {
                let live_messages = if session_is_active {
                    self.pi_manager.get_message_buffer(&req.session_id).await
                } else {
                    None
                };
                let (messages, source) = select_live_workspace_chat_messages(live_messages);
                (messages, source, WorkspaceChatMessagesSource::Live)
            }
        };

        debug!(
            "get_workspace_chat_session_messages session={} active={} requested_source={:?} selected_source={} count={}",
            req.session_id,
            session_is_active,
            req.source,
            source_label,
            messages.len()
        );

        RunnerResponse::WorkspaceChatSessionMessages(WorkspaceChatSessionMessagesResponse {
            session_id: req.session_id,
            source: source_mode,
            messages,
        })
    }

    // ========================================================================
    // Memory Operations (user-plane)
    // ========================================================================

    async fn list_memories(&self, req: ListMemoriesRequest) -> RunnerResponse {
        let (offset, limit) = (req.offset, req.limit);
        match tokio::task::spawn_blocking(move || -> anyhow::Result<(Vec<MemoryEntry>, usize)> {
            let file = open_workspace_memory(&req.workspace_path)?;
            let all = file.active_memories()?;
            let total = all.len();
            let window = all
                .into_iter()
                .skip(req.offset)
                .take(req.limit)
                .map(|m| memory_entry_to_wire(m, None))
                .collect();
            Ok((window, total))
        })
        .await
        {
            Ok(Ok((memories, total))) => RunnerResponse::MemoryList(MemoryListResponse {
                memories,
                total,
                offset,
                limit,
            }),
            Ok(Err(e)) => error_response(ErrorCode::Internal, format!("list memories failed: {e}")),
            Err(e) => error_response(
                ErrorCode::Internal,
                format!("list memories join error: {e}"),
            ),
        }
    }

    async fn search_memories(&self, req: SearchMemoriesRequest) -> RunnerResponse {
        match tokio::task::spawn_blocking(move || -> anyhow::Result<(String, Vec<MemoryEntry>)> {
            let file = open_workspace_memory(&req.workspace_path)?;
            let hits = file.search(&req.query, req.limit)?;
            let memories = hits
                .into_iter()
                .map(|h| memory_entry_to_wire(h.memory, Some(h.score as f64)))
                .collect();
            Ok((req.query, memories))
        })
        .await
        {
            Ok(Ok((query, memories))) => {
                let total = memories.len();
                RunnerResponse::MemorySearchResults(MemorySearchResultsResponse {
                    query,
                    memories,
                    total,
                })
            }
            Ok(Err(e)) => {
                error_response(ErrorCode::Internal, format!("search memories failed: {e}"))
            }
            Err(e) => error_response(
                ErrorCode::Internal,
                format!("search memories join error: {e}"),
            ),
        }
    }

    async fn add_memory(&self, req: AddMemoryRequest) -> RunnerResponse {
        match tokio::task::spawn_blocking(move || -> anyhow::Result<MemoryEntry> {
            let file = open_workspace_memory(&req.workspace_path)?;
            let mut event = mmry_core::memory_file::MemoryEvent::add(
                req.content,
                parse_wire_memory_type(req.memory_type.as_deref()),
                req.tags,
                &mmry_core::agent_ctx::AgentCtx::from_env(),
            );
            apply_memory_metadata(&mut event, req.category, req.importance);
            file.append(&event)?;
            find_wire_memory(&file, &event.memory_id)
        })
        .await
        {
            Ok(Ok(memory)) => RunnerResponse::MemoryAdded(MemoryAddedResponse { memory }),
            Ok(Err(e)) => error_response(ErrorCode::Internal, format!("add memory failed: {e}")),
            Err(e) => error_response(ErrorCode::Internal, format!("add memory join error: {e}")),
        }
    }

    async fn update_memory(&self, req: UpdateMemoryRequest) -> RunnerResponse {
        match tokio::task::spawn_blocking(move || -> anyhow::Result<MemoryEntry> {
            let file = open_workspace_memory(&req.workspace_path)?;
            let ctx = mmry_core::agent_ctx::AgentCtx::from_env();
            file.append(&mmry_core::memory_file::MemoryEvent::deprecate(
                req.memory_id,
                &ctx,
            ))?;
            let mut event = mmry_core::memory_file::MemoryEvent::add(
                req.content,
                parse_wire_memory_type(req.memory_type.as_deref()),
                req.tags,
                &ctx,
            );
            apply_memory_metadata(&mut event, req.category, req.importance);
            file.append(&event)?;
            find_wire_memory(&file, &event.memory_id)
        })
        .await
        {
            Ok(Ok(memory)) => RunnerResponse::MemoryUpdated(MemoryAddedResponse { memory }),
            Ok(Err(e)) => error_response(ErrorCode::Internal, format!("update memory failed: {e}")),
            Err(e) => error_response(
                ErrorCode::Internal,
                format!("update memory join error: {e}"),
            ),
        }
    }

    async fn delete_memory(&self, req: DeleteMemoryRequest) -> RunnerResponse {
        match tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
            let file = open_workspace_memory(&req.workspace_path)?;
            file.append(&mmry_core::memory_file::MemoryEvent::deprecate(
                req.memory_id.clone(),
                &mmry_core::agent_ctx::AgentCtx::from_env(),
            ))?;
            Ok(req.memory_id)
        })
        .await
        {
            Ok(Ok(memory_id)) => RunnerResponse::MemoryDeleted(MemoryDeletedResponse { memory_id }),
            Ok(Err(e)) => error_response(ErrorCode::Internal, format!("delete memory failed: {e}")),
            Err(e) => error_response(
                ErrorCode::Internal,
                format!("delete memory join error: {e}"),
            ),
        }
    }

    // ========================================================================
    // TRX (Issue Tracking) Operations (in-process via trx-core)
    // ========================================================================

    async fn trx_list(&self, req: TrxListRequest) -> RunnerResponse {
        match tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<TrxIssueData>> {
            let requested_path = req.workspace_path.clone();
            let resolved_root =
                find_trx_root(&requested_path).unwrap_or_else(|| requested_path.clone());
            tracing::info!(
                requested_workspace = %requested_path.display(),
                resolved_trx_root = %resolved_root.display(),
                "runner trx_list resolving store"
            );
            let store = open_or_init_trx_store(&requested_path)?;
            let issues: Vec<TrxIssueData> = store
                .list(false)
                .into_iter()
                .map(trx_issue_to_data)
                .collect();
            tracing::info!(
                requested_workspace = %requested_path.display(),
                issue_count = issues.len(),
                "runner trx_list loaded issues"
            );
            Ok(issues)
        })
        .await
        {
            Ok(Ok(issues)) => RunnerResponse::TrxList(TrxListResponse { issues }),
            Ok(Err(e)) => error_response(ErrorCode::Internal, format!("trx list failed: {e}")),
            Err(e) => error_response(ErrorCode::Internal, format!("trx list join error: {e}")),
        }
    }

    async fn trx_create(&self, req: TrxCreateRequest) -> RunnerResponse {
        match tokio::task::spawn_blocking(move || -> anyhow::Result<TrxIssueData> {
            use std::str::FromStr;
            let mut store = open_or_init_trx_store(&req.workspace_path)?;

            let id = match req.parent_id.as_deref() {
                Some(parent_id) => {
                    let n = store.next_child_num(parent_id);
                    trx_core::id::generate_child_id(parent_id, n)
                }
                None => {
                    let prefix = store.prefix()?;
                    trx_core::generate_id(&prefix)
                }
            };

            let mut issue = trx_core::Issue::new(id, req.title);
            issue.description = req.description;
            issue.priority = u8::try_from(req.priority).unwrap_or(2);
            issue.issue_type =
                trx_core::IssueType::from_str(&req.issue_type).unwrap_or(trx_core::IssueType::Task);

            if let Some(parent_id) = req.parent_id {
                issue.add_dependency(parent_id, trx_core::DependencyType::ParentChild);
            }

            store.create(issue.clone())?;
            Ok(trx_issue_to_data(&issue))
        })
        .await
        {
            Ok(Ok(issue)) => RunnerResponse::TrxIssue(TrxIssueResponse { issue }),
            Ok(Err(e)) => error_response(ErrorCode::Internal, format!("trx create failed: {e}")),
            Err(e) => error_response(ErrorCode::Internal, format!("trx create join error: {e}")),
        }
    }

    async fn trx_update(&self, req: TrxUpdateRequest) -> RunnerResponse {
        match tokio::task::spawn_blocking(move || -> anyhow::Result<TrxIssueData> {
            use std::str::FromStr;
            let mut store = open_or_init_trx_store(&req.workspace_path)?;

            let mut issue = store
                .get(&req.issue_id)
                .ok_or_else(|| anyhow::anyhow!("issue not found: {}", req.issue_id))?
                .clone();

            if let Some(title) = req.title {
                issue.title = title;
            }
            if let Some(description) = req.description {
                issue.description = Some(description);
            }
            if let Some(status) = req.status {
                issue.status = trx_core::Status::from_str(&status)
                    .map_err(|e| anyhow::anyhow!("invalid status: {e}"))?;
            }
            if let Some(priority) = req.priority {
                issue.priority = u8::try_from(priority).unwrap_or(issue.priority);
            }
            issue.updated_at = chrono::Utc::now();

            store.update(issue.clone())?;
            Ok(trx_issue_to_data(&issue))
        })
        .await
        {
            Ok(Ok(issue)) => RunnerResponse::TrxIssue(TrxIssueResponse { issue }),
            Ok(Err(e)) => error_response(ErrorCode::Internal, format!("trx update failed: {e}")),
            Err(e) => error_response(ErrorCode::Internal, format!("trx update join error: {e}")),
        }
    }

    async fn trx_close(&self, req: TrxCloseRequest) -> RunnerResponse {
        match tokio::task::spawn_blocking(move || -> anyhow::Result<TrxIssueData> {
            let mut store = open_or_init_trx_store(&req.workspace_path)?;

            let mut issue = store
                .get(&req.issue_id)
                .ok_or_else(|| anyhow::anyhow!("issue not found: {}", req.issue_id))?
                .clone();

            issue.close(req.reason);
            store.update(issue.clone())?;
            Ok(trx_issue_to_data(&issue))
        })
        .await
        {
            Ok(Ok(issue)) => RunnerResponse::TrxIssue(TrxIssueResponse { issue }),
            Ok(Err(e)) => error_response(ErrorCode::Internal, format!("trx close failed: {e}")),
            Err(e) => error_response(ErrorCode::Internal, format!("trx close join error: {e}")),
        }
    }

    /// Update a workspace chat session title.
    ///
    /// Pi JSONL remains the source of truth for harness session metadata, so a
    /// manual rename must reach Pi or append a `session_info` entry before any
    /// compatibility projection is updated.
    async fn update_workspace_chat_session(
        &self,
        req: UpdateWorkspaceChatSessionRequest,
    ) -> RunnerResponse {
        let Some(title) = req.title else {
            return error_response(ErrorCode::InvalidRequest, "No update fields provided");
        };

        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let external_session_id = oqto_history::oqto_log::ops::find_external_by_session(
            std::path::Path::new(&home),
            &req.session_id,
        )
        .await
        .unwrap_or_else(|| req.session_id.clone());

        let live_set_name_applied = self
            .pi_manager
            .set_session_name(&req.session_id, &title)
            .await
            .is_ok();
        if !live_set_name_applied {
            match append_session_info_name_for_external_id(&external_session_id, &title).await {
                Ok(true) => {}
                Ok(false) => {
                    return error_response(
                        ErrorCode::SessionNotFound,
                        format!(
                            "Session {} not found in live runner or Pi JSONL",
                            req.session_id
                        ),
                    );
                }
                Err(e) => {
                    return error_response(
                        ErrorCode::IoError,
                        format!("Failed to append session rename to Pi JSONL: {e}"),
                    );
                }
            }
        }

        let _ = oqto_history::oqto_log::ops::update_session_title(
            std::path::Path::new(&home),
            &req.session_id,
            &title,
        )
        .await;
        if external_session_id != req.session_id {
            let _ = oqto_history::oqto_log::ops::update_session_title(
                std::path::Path::new(&home),
                &external_session_id,
                &title,
            )
            .await;
        }

        match self
            .get_workspace_chat_session(GetWorkspaceChatSessionRequest {
                session_id: req.session_id.clone(),
            })
            .await
        {
            RunnerResponse::WorkspaceChatSession(WorkspaceChatSessionResponse {
                session: Some(mut session),
            }) => {
                session.title = Some(title);
                RunnerResponse::WorkspaceChatSessionUpdated(WorkspaceChatSessionUpdatedResponse {
                    session,
                })
            }
            _ => RunnerResponse::WorkspaceChatSessionUpdated(WorkspaceChatSessionUpdatedResponse {
                session: WorkspaceChatSessionInfo {
                    id: req.session_id,
                    readable_id: String::new(),
                    title: Some(title),
                    parent_id: None,
                    workspace_path: "global".to_string(),
                    project_name: "global".to_string(),
                    created_at: 0,
                    updated_at: chrono::Utc::now().timestamp_millis(),
                    version: None,
                    is_child: false,
                    model: None,
                    provider: None,
                },
            }),
        }
    }

    /// Repair missing workspace chat history by scanning Pi JSONL session files
    /// and syncing them to oqto-log (the authoritative store).
    async fn repair_workspace_chat_history(
        &self,
        req: RepairWorkspaceChatHistoryRequest,
    ) -> RunnerResponse {
        let scan =
            match tokio::task::spawn_blocking(move || scan_pi_jsonl_session_metadata(req.limit))
                .await
            {
                Ok(outcome) => outcome,
                Err(err) => {
                    return error_response(
                        ErrorCode::Internal,
                        format!("Failed to scan Pi session files: {err}"),
                    );
                }
            };

        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => {
                return error_response(ErrorCode::Internal, "HOME not set");
            }
        };
        let user_id = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
        let home_path = std::path::Path::new(&home);

        let mut repaired = 0usize;
        let mut skipped = scan.skipped_files;
        let mut failed = scan.failed_files;

        for session in scan.sessions {
            if let Some(workspace_filter) = req.workspace.as_ref() {
                let matches_workspace = session.workspace_path.as_ref().is_some_and(|path| {
                    path == workspace_filter || path.starts_with(&format!("{workspace_filter}/"))
                });
                if !matches_workspace {
                    skipped += 1;
                    continue;
                }
            }

            let records = read_jsonl_message_records(&session.session_file);
            let recovered_messages: Vec<_> = records
                .iter()
                .map(|record| record.message.clone())
                .collect();
            if recovered_messages.is_empty() {
                skipped += 1;
                continue;
            }

            let workspace_id = session.workspace_path.as_deref().unwrap_or("global");

            // Look up existing oqto-log session by external_id to find the
            // correct session_id and workspace (oqto IDs differ from Pi IDs).
            let (oqto_session_id, workspace_id) =
                match oqto_history::oqto_log::ops::find_session_by_external(
                    home_path,
                    &session.external_id,
                )
                .await
                {
                    Some((id, ws)) if !ws.is_empty() => (id, ws.as_str().to_owned()),
                    Some((id, _)) => (id, workspace_id.to_owned()),
                    None => (session.external_id.clone(), workspace_id.to_owned()),
                };
            let workspace_id = workspace_id.as_str();

            // Append new messages (dedup handles already-persisted ones).
            match oqto_history::oqto_log::store::append_agent_end_snapshot(
                home_path,
                &user_id,
                workspace_id,
                &oqto_session_id,
                &oqto_session_id,
                Some(&session.external_id),
                &session.external_id,
                &recovered_messages,
            )
            .await
            {
                Ok(stats) => {
                    // Self-heal: if oqto-log still has fewer messages than the
                    // JSONL, replace the whole session deterministically.
                    if let Ok(sess_stats) = oqto_history::oqto_log::store::read_session_stats(
                        home_path,
                        workspace_id,
                        &oqto_session_id,
                    )
                    .await
                        && sess_stats.messages < recovered_messages.len()
                    {
                        match oqto_history::oqto_log::store::replace_session_with_pi_jsonl_records(
                            home_path,
                            &user_id,
                            workspace_id,
                            &oqto_session_id,
                            &oqto_session_id,
                            Some(&session.external_id),
                            &session.external_id,
                            &records,
                        )
                        .await
                        {
                            Ok(_) => {
                                info!(
                                    "oqto-log self-healed session {} ({} -> {} messages)",
                                    oqto_session_id,
                                    sess_stats.messages,
                                    recovered_messages.len()
                                );
                                repaired += 1;
                                continue;
                            }
                            Err(e) => {
                                warn!(
                                    "oqto-log replace failed for session {}: {:?}",
                                    oqto_session_id, e
                                );
                                failed += 1;
                                continue;
                            }
                        }
                    }
                    if stats.turns_written > 0 {
                        repaired += 1;
                    } else {
                        skipped += 1;
                    }
                }
                Err(e) => {
                    warn!(
                        "oqto-log append failed for session {}: {:?}",
                        oqto_session_id, e
                    );
                    failed += 1;
                }
            }
        }

        info!(
            "workspace chat history repair completed: scanned={} repaired={} skipped={} failed={}",
            scan.scanned_files, repaired, skipped, failed
        );

        RunnerResponse::WorkspaceChatHistoryRepaired(WorkspaceChatHistoryRepairResponse {
            scanned_files: scan.scanned_files,
            repaired_conversations: repaired,
            skipped_files: skipped,
            failed_files: failed,
        })
    }

    // Pi Session Management Operations
    // ========================================================================

    /// Create or resume a Pi session.
    async fn pi_create_session(&self, req: PiCreateSessionRequest) -> RunnerResponse {
        info!(
            "pi_create_session: session_id={}, cwd={:?}",
            req.session_id, req.config.cwd
        );

        // Convert protocol config to pi_manager config
        let pi_config = crate::pi_manager::PiSessionConfig {
            cwd: req.config.cwd,
            provider: req.config.provider,
            model: req.config.model,
            session_file: req.config.session_file,
            continue_session: req.config.continue_session,
            env: req.config.env,
        };

        match self
            .pi_manager
            .get_or_create_session(&req.session_id, pi_config)
            .await
        {
            Ok(real_session_id) => RunnerResponse::PiSessionCreated(PiSessionCreatedResponse {
                session_id: real_session_id,
            }),
            Err(e) => error_response(
                ErrorCode::Internal,
                format!("Failed to create Pi session: {}", e),
            ),
        }
    }

    /// Send a prompt to a Pi session.
    async fn pi_prompt(&self, req: PiPromptRequest) -> RunnerResponse {
        debug!(
            "pi_prompt: session_id={}, message_len={}, client_id={:?}",
            req.session_id,
            req.message.len(),
            req.client_id
        );

        match self
            .pi_manager
            .prompt(&req.session_id, &req.message, req.client_id.clone())
            .await
        {
            Ok(()) => RunnerResponse::PiCommandAck {
                session_id: req.session_id,
            },
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to send prompt: {}", e),
            ),
        }
    }

    /// Send a steering message to interrupt a Pi session.
    async fn pi_steer(&self, req: PiSteerRequest) -> RunnerResponse {
        debug!(
            "pi_steer: session_id={}, message_len={}, client_id={:?}",
            req.session_id,
            req.message.len(),
            req.client_id,
        );

        match self
            .pi_manager
            .steer_with_client_id(&req.session_id, &req.message, req.client_id)
            .await
        {
            Ok(()) => RunnerResponse::PiCommandAck {
                session_id: req.session_id,
            },
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to send steer: {}", e),
            ),
        }
    }

    /// Queue a follow-up message for a Pi session.
    async fn pi_follow_up(&self, req: PiFollowUpRequest) -> RunnerResponse {
        debug!(
            "pi_follow_up: session_id={}, message_len={}, client_id={:?}",
            req.session_id,
            req.message.len(),
            req.client_id,
        );

        match self
            .pi_manager
            .follow_up_with_client_id(&req.session_id, &req.message, req.client_id)
            .await
        {
            Ok(()) => RunnerResponse::PiCommandAck {
                session_id: req.session_id,
            },
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to send follow_up: {}", e),
            ),
        }
    }

    /// Abort a Pi session's current operation.
    async fn pi_abort(&self, req: PiAbortRequest) -> RunnerResponse {
        debug!("pi_abort: session_id={}", req.session_id);

        match self.pi_manager.abort(&req.session_id).await {
            Ok(()) => RunnerResponse::PiCommandAck {
                session_id: req.session_id,
            },
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to abort: {}", e),
            ),
        }
    }

    /// Compact a Pi session's conversation.
    async fn pi_compact(&self, req: PiCompactRequest) -> RunnerResponse {
        debug!(
            "pi_compact: session_id={}, has_instructions={}",
            req.session_id,
            req.instructions.is_some()
        );

        match self
            .pi_manager
            .compact(&req.session_id, req.instructions.as_deref())
            .await
        {
            Ok(()) => RunnerResponse::PiCommandAck {
                session_id: req.session_id,
            },
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to compact: {}", e),
            ),
        }
    }

    /// Unsubscribe from a Pi session's events.
    /// Note: Actual unsubscription happens when the broadcast receiver is dropped.
    /// This is just an acknowledgment.
    async fn pi_unsubscribe(&self, req: PiUnsubscribeRequest) -> RunnerResponse {
        debug!("pi_unsubscribe: session_id={}", req.session_id);
        // The actual unsubscription happens when the receiver is dropped on the client side
        // This just acknowledges the request
        RunnerResponse::Ok
    }

    /// List all active Pi sessions.
    async fn pi_list_sessions(&self) -> RunnerResponse {
        debug!("pi_list_sessions");
        let sessions = self.pi_manager.list_sessions().await;
        RunnerResponse::PiSessionList(PiSessionListResponse { sessions })
    }

    /// Get the state of a Pi session.
    async fn pi_get_state(&self, req: PiGetStateRequest) -> RunnerResponse {
        debug!("pi_get_state: session_id={}", req.session_id);

        match self.pi_manager.get_state(&req.session_id).await {
            Ok(state) => RunnerResponse::PiState(PiStateResponse {
                session_id: req.session_id,
                state,
            }),
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to get state: {}", e),
            ),
        }
    }

    /// Close a Pi session.
    async fn pi_close_session(&self, req: PiCloseSessionRequest) -> RunnerResponse {
        info!("pi_close_session: session_id={}", req.session_id);

        match self.pi_manager.close_session(&req.session_id).await {
            Ok(()) => RunnerResponse::PiSessionClosed {
                session_id: req.session_id,
            },
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to close session: {}", e),
            ),
        }
    }

    /// Delete a Pi session: close the process, remove from oqto-log, and delete the JSONL file.
    async fn pi_delete_session(&self, req: PiDeleteSessionRequest) -> RunnerResponse {
        info!("pi_delete_session: session_id={}", req.session_id);

        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let external_id = oqto_history::oqto_log::ops::find_external_by_session(
            std::path::Path::new(&home),
            &req.session_id,
        )
        .await
        .unwrap_or_else(|| req.session_id.clone());

        let _ = self.pi_manager.close_session(&req.session_id).await;

        if let Err(e) = oqto_history::oqto_log::ops::delete_session(
            std::path::Path::new(&home),
            &req.session_id,
        )
        .await
        {
            warn!(
                "Failed to delete oqto-log session {}: {}",
                req.session_id, e
            );
        }

        let sessions_dir = dirs::home_dir()
            .unwrap_or_default()
            .join(".pi/agent/sessions");
        if sessions_dir.is_dir()
            && let Ok(entries) = std::fs::read_dir(&sessions_dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if let Ok(files) = std::fs::read_dir(&path) {
                    for file in files.flatten() {
                        let fname = file.file_name();
                        let fname_str = fname.to_string_lossy();
                        if fname_str.ends_with(".jsonl")
                            && (fname_str.contains(&external_id)
                                || fname_str.contains(&req.session_id))
                        {
                            info!("Deleting Pi session file: {}", file.path().display());
                            let _ = std::fs::remove_file(file.path());
                        }
                    }
                }
            }
        }

        RunnerResponse::PiSessionDeleted {
            session_id: req.session_id,
        }
    }

    /// Start a new session within existing Pi process.
    async fn pi_new_session(&self, req: PiNewSessionRequest) -> RunnerResponse {
        debug!(
            "pi_new_session: session_id={}, parent={:?}",
            req.session_id, req.parent_session
        );

        match self
            .pi_manager
            .new_session(&req.session_id, req.parent_session.as_deref())
            .await
        {
            Ok(()) => RunnerResponse::PiCommandAck {
                session_id: req.session_id,
            },
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to create new session: {}", e),
            ),
        }
    }

    /// Switch to a different session file.
    async fn pi_switch_session(&self, req: PiSwitchSessionRequest) -> RunnerResponse {
        debug!(
            "pi_switch_session: session_id={}, path={}",
            req.session_id, req.session_path
        );

        match self
            .pi_manager
            .switch_session(&req.session_id, &req.session_path)
            .await
        {
            Ok(()) => RunnerResponse::PiCommandAck {
                session_id: req.session_id,
            },
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to switch session: {}", e),
            ),
        }
    }

    /// Get all messages from a Pi session.
    async fn pi_get_messages(&self, req: PiGetMessagesRequest) -> RunnerResponse {
        debug!("pi_get_messages: session_id={}", req.session_id);

        // Prefer the in-memory message buffer (authoritative for active sessions).
        // Falls back to Pi RPC only if the buffer is empty (session just started).
        if let Some(buffer) = self.pi_manager.get_message_buffer(&req.session_id).await
            && !buffer.is_empty()
        {
            // Convert projected chat messages back to AgentMessage for legacy protocol compat.
            let agent_msgs: Vec<oqto_pi::AgentMessage> = buffer
                .into_iter()
                .map(crate::protocol::chat_proto_to_agent_msg)
                .collect();
            return RunnerResponse::PiMessages(PiMessagesResponse {
                session_id: req.session_id,
                messages: agent_msgs,
            });
        }

        // Buffer empty -- fall back to Pi RPC
        match self.pi_manager.get_messages(&req.session_id).await {
            Ok(messages) => {
                let messages_vec: Vec<oqto_pi::AgentMessage> = messages
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| serde_json::from_value(v.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default();
                RunnerResponse::PiMessages(PiMessagesResponse {
                    session_id: req.session_id,
                    messages: messages_vec,
                })
            }
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to get messages: {}", e),
            ),
        }
    }

    /// Get session statistics.
    async fn pi_get_session_stats(&self, req: PiGetSessionStatsRequest) -> RunnerResponse {
        debug!("pi_get_session_stats: session_id={}", req.session_id);

        match self.pi_manager.get_session_stats(&req.session_id).await {
            Ok(stats) => RunnerResponse::PiSessionStats(PiSessionStatsResponse {
                session_id: req.session_id,
                stats,
            }),
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to get session stats: {}", e),
            ),
        }
    }

    /// Get the last assistant message text.
    async fn pi_get_last_assistant_text(
        &self,
        req: PiGetLastAssistantTextRequest,
    ) -> RunnerResponse {
        debug!("pi_get_last_assistant_text: session_id={}", req.session_id);

        match self
            .pi_manager
            .get_last_assistant_text(&req.session_id)
            .await
        {
            Ok(text) => RunnerResponse::PiLastAssistantText(PiLastAssistantTextResponse {
                session_id: req.session_id,
                text,
            }),
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to get last assistant text: {}", e),
            ),
        }
    }

    /// Parse model info from a Pi command response.
    ///
    /// Pi responses vary by command/version. We support:
    /// - { model: "<id>", provider: "<provider>" }
    /// - { model_id: "<id>", provider: "<provider>" }
    /// - { model: { id: "<id>", provider: "<provider>", ... } }
    fn parse_model_from_response(response: &oqto_pi::PiResponse) -> Option<oqto_pi::PiModel> {
        let data = response.data.as_ref()?;

        // Nested model object variant: { model: { id, provider, ... } }
        if let Some(model_obj) = data.get("model").and_then(|v| v.as_object()) {
            let model_id = model_obj.get("id").and_then(|v| v.as_str())?;
            let provider = model_obj
                .get("provider")
                .and_then(|v| v.as_str())
                .or_else(|| model_obj.get("api").and_then(|v| v.as_str()))?;
            return Some(oqto_pi::PiModel {
                id: model_id.to_string(),
                name: model_obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(model_id)
                    .to_string(),
                api: provider.to_string(),
                provider: provider.to_string(),
                base_url: None,
                reasoning: false,
                input: vec!["text".to_string()],
                context_window: 0,
                max_tokens: 0,
                cost: None,
            });
        }

        // Flat variants
        let model_id = data
            .get("model")
            .and_then(|v| v.as_str())
            .or_else(|| data.get("model_id").and_then(|v| v.as_str()))?;
        let provider = data
            .get("provider")
            .and_then(|v| v.as_str())
            .or_else(|| data.get("provider_id").and_then(|v| v.as_str()))?;

        Some(oqto_pi::PiModel {
            id: model_id.to_string(),
            name: model_id.to_string(),
            api: provider.to_string(),
            provider: provider.to_string(),
            base_url: None,
            reasoning: false,
            input: vec!["text".to_string()],
            context_window: 0,
            max_tokens: 0,
            cost: None,
        })
    }

    /// Resolve the post-command active model from Pi.
    ///
    /// The source of truth is `get_state.model`; response payload parsing is a
    /// best-effort fallback for older Pi variants.
    async fn resolve_authoritative_model(
        &self,
        session_id: &str,
        response: &oqto_pi::PiResponse,
    ) -> Option<oqto_pi::PiModel> {
        match self.pi_manager.get_state(session_id).await {
            Ok(state) => {
                if let Some(model) = state.model {
                    return Some(model);
                }
                warn!(
                    "pi model change verification: get_state returned no model for session {}",
                    session_id
                );
            }
            Err(err) => {
                warn!(
                    "pi model change verification: get_state failed for session {}: {}",
                    session_id, err
                );
            }
        }

        Self::parse_model_from_response(response)
    }

    async fn pi_set_model(&self, req: PiSetModelRequest) -> RunnerResponse {
        debug!(
            "pi_set_model: session_id={}, provider={}, model_id={}",
            req.session_id, req.provider, req.model_id
        );

        match self
            .pi_manager
            .set_model(&req.session_id, &req.provider, &req.model_id)
            .await
        {
            Ok(response) => {
                let Some(model) = self
                    .resolve_authoritative_model(&req.session_id, &response)
                    .await
                else {
                    return error_response(
                        ErrorCode::Internal,
                        "set_model succeeded but active model could not be verified".to_string(),
                    );
                };

                if let Err(err) = self
                    .pi_manager
                    .set_session_model_cache(&req.session_id, &model.provider, &model.id)
                    .await
                {
                    warn!(
                        "Failed to update model cache after set_model for session {}: {}",
                        req.session_id, err
                    );
                }

                RunnerResponse::PiModelChanged(PiModelChangedResponse {
                    session_id: req.session_id,
                    model,
                    thinking_level: String::new(),
                    is_scoped: false,
                })
            }
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to set model: {}", e),
            ),
        }
    }

    /// Cycle to the next available model.
    async fn pi_cycle_model(&self, req: PiCycleModelRequest) -> RunnerResponse {
        debug!("pi_cycle_model: session_id={}", req.session_id);

        match self.pi_manager.cycle_model(&req.session_id).await {
            Ok(response) => {
                let Some(model) = self
                    .resolve_authoritative_model(&req.session_id, &response)
                    .await
                else {
                    return error_response(
                        ErrorCode::Internal,
                        "cycle_model succeeded but active model could not be verified".to_string(),
                    );
                };

                if let Err(err) = self
                    .pi_manager
                    .set_session_model_cache(&req.session_id, &model.provider, &model.id)
                    .await
                {
                    warn!(
                        "Failed to update model cache after cycle_model for session {}: {}",
                        req.session_id, err
                    );
                }

                RunnerResponse::PiModelChanged(PiModelChangedResponse {
                    session_id: req.session_id,
                    model,
                    thinking_level: String::new(),
                    is_scoped: false,
                })
            }
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to cycle model: {}", e),
            ),
        }
    }

    /// Get available models.
    async fn pi_get_available_models(&self, req: PiGetAvailableModelsRequest) -> RunnerResponse {
        debug!(
            "pi_get_available_models: session_id={}, workdir={:?}",
            req.session_id, req.workdir
        );

        match self
            .pi_manager
            .get_available_models(&req.session_id, req.workdir.as_deref())
            .await
        {
            Ok(models) => {
                // pi_manager now returns a flat array, but handle object wrapper as fallback
                let models_arr = if models.is_array() {
                    &models
                } else if let Some(inner) = models.get("models") {
                    inner
                } else {
                    &models
                };
                let models_vec: Vec<oqto_pi::PiModel> = models_arr
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| {
                                match serde_json::from_value::<oqto_pi::PiModel>(v.clone()) {
                                    Ok(m) => Some(m),
                                    Err(e) => {
                                        let provider = v
                                            .get("provider")
                                            .and_then(|p| p.as_str())
                                            .unwrap_or("?");
                                        let id =
                                            v.get("id").and_then(|i| i.as_str()).unwrap_or("?");
                                        warn!(
                                            "Failed to deserialize model {}/{}: {}",
                                            provider, id, e
                                        );
                                        None
                                    }
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                RunnerResponse::PiAvailableModels(PiAvailableModelsResponse {
                    session_id: req.session_id,
                    models: models_vec,
                })
            }
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to get available models: {}", e),
            ),
        }
    }

    /// Set the thinking level.
    async fn pi_set_thinking_level(&self, req: PiSetThinkingLevelRequest) -> RunnerResponse {
        debug!(
            "pi_set_thinking_level: session_id={}, level={}",
            req.session_id, req.level
        );

        match self
            .pi_manager
            .set_thinking_level(&req.session_id, &req.level)
            .await
        {
            Ok(response) => {
                let level = response
                    .data
                    .as_ref()
                    .and_then(|d| d.get("level"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&req.level)
                    .to_string();
                RunnerResponse::PiThinkingLevelChanged(PiThinkingLevelChangedResponse {
                    session_id: req.session_id,
                    level,
                })
            }
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to set thinking level: {}", e),
            ),
        }
    }

    /// Cycle through thinking levels.
    async fn pi_cycle_thinking_level(&self, req: PiCycleThinkingLevelRequest) -> RunnerResponse {
        debug!("pi_cycle_thinking_level: session_id={}", req.session_id);

        match self.pi_manager.cycle_thinking_level(&req.session_id).await {
            Ok(response) => {
                let level = response
                    .data
                    .as_ref()
                    .and_then(|d| d.get("level"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("medium")
                    .to_string();
                RunnerResponse::PiThinkingLevelChanged(PiThinkingLevelChangedResponse {
                    session_id: req.session_id,
                    level,
                })
            }
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to cycle thinking level: {}", e),
            ),
        }
    }

    /// Enable/disable auto-compaction.
    async fn pi_set_auto_compaction(&self, req: PiSetAutoCompactionRequest) -> RunnerResponse {
        debug!(
            "pi_set_auto_compaction: session_id={}, enabled={}",
            req.session_id, req.enabled
        );

        match self
            .pi_manager
            .set_auto_compaction(&req.session_id, req.enabled)
            .await
        {
            Ok(()) => RunnerResponse::PiCommandAck {
                session_id: req.session_id,
            },
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to set auto compaction: {}", e),
            ),
        }
    }

    /// Set steering message delivery mode.
    async fn pi_set_steering_mode(&self, req: PiSetSteeringModeRequest) -> RunnerResponse {
        debug!(
            "pi_set_steering_mode: session_id={}, mode={}",
            req.session_id, req.mode
        );

        match self
            .pi_manager
            .set_steering_mode(&req.session_id, &req.mode)
            .await
        {
            Ok(()) => RunnerResponse::PiCommandAck {
                session_id: req.session_id,
            },
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to set steering mode: {}", e),
            ),
        }
    }

    /// Set follow-up message delivery mode.
    async fn pi_set_follow_up_mode(&self, req: PiSetFollowUpModeRequest) -> RunnerResponse {
        debug!(
            "pi_set_follow_up_mode: session_id={}, mode={}",
            req.session_id, req.mode
        );

        match self
            .pi_manager
            .set_follow_up_mode(&req.session_id, &req.mode)
            .await
        {
            Ok(()) => RunnerResponse::PiCommandAck {
                session_id: req.session_id,
            },
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to set follow up mode: {}", e),
            ),
        }
    }

    /// Enable/disable auto-retry.
    async fn pi_set_auto_retry(&self, req: PiSetAutoRetryRequest) -> RunnerResponse {
        debug!(
            "pi_set_auto_retry: session_id={}, enabled={}",
            req.session_id, req.enabled
        );

        match self
            .pi_manager
            .set_auto_retry(&req.session_id, req.enabled)
            .await
        {
            Ok(()) => RunnerResponse::PiCommandAck {
                session_id: req.session_id,
            },
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to set auto retry: {}", e),
            ),
        }
    }

    /// Abort an in-progress retry.
    async fn pi_abort_retry(&self, req: PiAbortRetryRequest) -> RunnerResponse {
        debug!("pi_abort_retry: session_id={}", req.session_id);

        match self.pi_manager.abort_retry(&req.session_id).await {
            Ok(()) => RunnerResponse::PiCommandAck {
                session_id: req.session_id,
            },
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to abort retry: {}", e),
            ),
        }
    }

    /// Fork from a previous message.
    async fn pi_fork(&self, mut req: PiForkRequest) -> RunnerResponse {
        debug!(
            "pi_fork: session_id={}, entry_id={}",
            req.session_id, req.entry_id
        );

        // The chat projection exposes oqto-log message IDs for stable UI
        // identity, while Pi fork requires the native JSONL entry ID. Resolve
        // that adapter boundary inside the per-user runner, whose HOME owns
        // the authoritative oqto-log shard. Native source IDs pass through.
        if let Ok(home) = std::env::var("HOME")
            && let Some(source_entry_id) = oqto_history::oqto_log::ops::resolve_source_entry_id(
                std::path::Path::new(&home),
                &req.session_id,
                &req.entry_id,
            )
            .await
        {
            debug!(
                "pi_fork: resolved oqto-log entry {} to native source entry {}",
                req.entry_id, source_entry_id
            );
            req.entry_id = source_entry_id;
        }

        // Fail closed for non-Pi synthetic IDs. Fork must target a real
        // Pi entry identifier from get_fork_messages.
        if req.entry_id.starts_with("history-") {
            return error_response(
                ErrorCode::PiSessionInvalidState,
                format!(
                    "Invalid fork entry_id '{}' (frontend sent synthetic history id). Reload fork points and retry.",
                    req.entry_id
                ),
            );
        }

        // Transactional fork guard: exactly one in-flight fork per session.
        let _fork_txn_guard = match self
            .pi_manager
            .try_begin_fork_transaction(&req.session_id)
            .await
        {
            Ok(Some(permit)) => permit,
            Ok(None) => {
                return error_response(
                    ErrorCode::PiSessionInvalidState,
                    "Fork already in progress for this session",
                );
            }
            Err(e) => {
                return error_response(
                    ErrorCode::PiSessionNotFound,
                    format!("Failed to start fork transaction: {}", e),
                );
            }
        };

        let old_config = match self.pi_manager.get_session_config(&req.session_id).await {
            Some(cfg) => cfg,
            None => {
                return error_response(
                    ErrorCode::PiSessionNotFound,
                    format!("Fork parent session is not active: {}", req.session_id),
                );
            }
        };
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let home_path = std::path::Path::new(&home);
        let parent_row =
            match oqto_history::oqto_log::ops::get_session(home_path, &req.session_id).await {
                Ok(Some(row)) => row,
                Ok(None) => {
                    return error_response(
                        ErrorCode::Internal,
                        format!("Fork parent is missing from oqto-log: {}", req.session_id),
                    );
                }
                Err(e) => {
                    return error_response(
                        ErrorCode::Internal,
                        format!("Failed to resolve fork parent in oqto-log: {e:#}"),
                    );
                }
            };
        let workspace_id = parent_row
            .workspace_id
            .clone()
            .unwrap_or_else(|| old_config.cwd.to_string_lossy().to_string());

        if let Some(operation_id) = req.operation_id.as_deref() {
            match oqto_history::oqto_log::store::find_session_fork_by_operation(
                home_path,
                &workspace_id,
                operation_id,
            )
            .await
            {
                Ok(Some((child_session_id, prompt_text))) => {
                    info!(
                        "Fork operation '{}' replayed; returning existing child '{}'",
                        operation_id, child_session_id
                    );
                    return RunnerResponse::PiForkResult(PiForkResultResponse {
                        session_id: req.session_id,
                        text: prompt_text,
                        cancelled: false,
                        new_session_id: Some(child_session_id),
                    });
                }
                Ok(None) => {}
                Err(e) => {
                    return error_response(
                        ErrorCode::Internal,
                        format!("Failed to resolve fork operation replay: {e:#}"),
                    );
                }
            }
        }

        let fork_result = match self.pi_manager.fork(&req.session_id, &req.entry_id).await {
            Ok(r) => r,
            Err(e) => {
                return error_response(
                    ErrorCode::PiSessionNotFound,
                    format!("Failed to fork: {}", e),
                );
            }
        };

        if fork_result.cancelled {
            return RunnerResponse::PiForkResult(PiForkResultResponse {
                session_id: req.session_id,
                text: fork_result.text,
                cancelled: true,
                new_session_id: None,
            });
        }

        let new_oqto_id = format!("oqto-{}", uuid::Uuid::new_v4());

        let fork_session_file = if let Some(ref file) = fork_result.new_session_file {
            Some(std::path::PathBuf::from(file))
        } else if let Some(ref pi_id) = fork_result.new_session_id {
            oqto_pi::session_files::find_session_file(pi_id, Some(&old_config.cwd))
        } else {
            None
        };

        let Some(fork_session_file) = fork_session_file else {
            return error_response(
                ErrorCode::Internal,
                "Fork succeeded but forked session file could not be resolved",
            );
        };

        let parent_session_file = match self.pi_manager.get_state(&req.session_id).await {
            Ok(state) => match state.session_file {
                Some(path) => std::path::PathBuf::from(path),
                None => {
                    return error_response(
                        ErrorCode::Internal,
                        "Fork succeeded but parent session file could not be resolved",
                    );
                }
            },
            Err(e) => {
                return error_response(
                    ErrorCode::Internal,
                    format!("Fork succeeded but parent state could not be reloaded: {e}"),
                );
            }
        };
        if let Err(e) = validate_fork_copy(&parent_session_file, &fork_session_file, &req.entry_id)
        {
            return error_response(
                ErrorCode::PiSessionInvalidState,
                format!("Fork copy verification failed: {e:#}"),
            );
        }

        // Make the child durable and readable before exposing its id. The
        // copied JSONL is Pi's authority; oqto-log stores the canonical child
        // identity, copied history, and immutable Fork provenance.
        let Some(child_external_id) = parse_pi_session_id_from_path(&fork_session_file) else {
            return error_response(
                ErrorCode::Internal,
                format!(
                    "Forked session file has no Pi session id: {}",
                    fork_session_file.display()
                ),
            );
        };
        let child_path_for_read = fork_session_file.clone();
        let records = match tokio::task::spawn_blocking(move || {
            read_jsonl_message_records(&child_path_for_read)
        })
        .await
        {
            Ok(records) => records,
            Err(e) => {
                return error_response(
                    ErrorCode::Internal,
                    format!("Failed to read forked session history: {e}"),
                );
            }
        };
        let user_id = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
        let imported = match oqto_history::oqto_log::store::replace_session_with_pi_jsonl_records(
            home_path,
            &user_id,
            &workspace_id,
            &new_oqto_id,
            &new_oqto_id,
            Some(&child_external_id),
            &child_external_id,
            &records,
        )
        .await
        {
            Ok(stats) => stats,
            Err(e) => {
                return error_response(
                    ErrorCode::Internal,
                    format!("Failed to persist forked session history: {e:#}"),
                );
            }
        };
        if let Err(e) = oqto_history::oqto_log::store::record_session_fork(
            home_path,
            &workspace_id,
            &imported.session_id,
            &parent_row.session_id,
            &req.entry_id,
            req.operation_id.as_deref(),
            Some(&fork_result.text),
        )
        .await
        {
            return error_response(
                ErrorCode::Internal,
                format!("Failed to persist fork lineage: {e:#}"),
            );
        }

        let mut child_config = old_config;
        child_config.session_file = Some(fork_session_file.clone());
        child_config.continue_session = None;

        // The durable Fork already succeeded. Starting its Pi process is a
        // readiness optimization; if it fails, navigation can resume the child
        // from its persisted JSONL instead of encouraging a duplicate retry.
        if let Err(e) = self
            .pi_manager
            .create_session(imported.session_id.clone(), child_config)
            .await
        {
            warn!(
                "Fork child '{}' persisted but eager Pi start failed: {e:#}",
                imported.session_id
            );
        }

        info!(
            "Fork: persisted child session '{}' (pi_id={}, file={}) from parent '{}' at entry '{}'",
            imported.session_id,
            child_external_id,
            fork_session_file.display(),
            req.session_id,
            req.entry_id
        );

        RunnerResponse::PiForkResult(PiForkResultResponse {
            session_id: req.session_id,
            text: fork_result.text,
            cancelled: false,
            new_session_id: Some(imported.session_id),
        })
    }

    /// Get messages available for forking.
    async fn pi_get_fork_messages(&self, req: PiGetForkMessagesRequest) -> RunnerResponse {
        debug!("pi_get_fork_messages: session_id={}", req.session_id);

        match self.pi_manager.get_fork_messages(&req.session_id).await {
            Ok(messages) => {
                // Parse JSON response to typed PiForkMessage vec.
                // Be liberal with accepted field names because Pi versions may
                // return e.g. entryId/preview instead of entry_id/text.
                let messages_arr = messages
                    .as_array()
                    .cloned()
                    .or_else(|| messages.get("messages").and_then(|m| m.as_array()).cloned())
                    .unwrap_or_default();

                let messages_vec: Vec<PiForkMessage> = messages_arr
                    .iter()
                    .filter_map(|v| {
                        let obj = v.as_object()?;
                        let entry_id = obj
                            .get("entry_id")
                            .and_then(|x| x.as_str())
                            .or_else(|| obj.get("entryId").and_then(|x| x.as_str()))
                            .or_else(|| obj.get("id").and_then(|x| x.as_str()))?
                            .to_string();

                        let text = obj
                            .get("text")
                            .and_then(|x| x.as_str())
                            .or_else(|| obj.get("preview").and_then(|x| x.as_str()))
                            .or_else(|| obj.get("content").and_then(|x| x.as_str()))
                            .or_else(|| {
                                obj.get("message")
                                    .and_then(|m| m.as_object())
                                    .and_then(|m| m.get("content"))
                                    .and_then(|c| c.as_str())
                            })
                            .unwrap_or("")
                            .to_string();

                        Some(PiForkMessage { entry_id, text })
                    })
                    .collect();
                RunnerResponse::PiForkMessages(PiForkMessagesResponse {
                    session_id: req.session_id,
                    messages: messages_vec,
                })
            }
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to get fork messages: {}", e),
            ),
        }
    }

    /// Set a display name for the session.
    async fn pi_set_session_name(&self, req: PiSetSessionNameRequest) -> RunnerResponse {
        debug!(
            "pi_set_session_name: session_id={}, name={}",
            req.session_id, req.name
        );

        match self
            .pi_manager
            .set_session_name(&req.session_id, &req.name)
            .await
        {
            Ok(()) => RunnerResponse::PiCommandAck {
                session_id: req.session_id,
            },
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to set session name: {}", e),
            ),
        }
    }

    /// Export session to HTML.
    async fn pi_export_html(&self, req: PiExportHtmlRequest) -> RunnerResponse {
        debug!(
            "pi_export_html: session_id={}, path={:?}",
            req.session_id, req.output_path
        );

        match self
            .pi_manager
            .export_html(&req.session_id, req.output_path.as_deref())
            .await
        {
            Ok(result) => {
                let path = result
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("/tmp/session.html")
                    .to_string();
                RunnerResponse::PiExportHtmlResult(PiExportHtmlResultResponse {
                    session_id: req.session_id,
                    path,
                })
            }
            Err(e) => error_response(ErrorCode::Internal, format!("Failed to export HTML: {}", e)),
        }
    }

    /// Return runner-advertised capabilities for backend negotiation.
    async fn get_capabilities(&self) -> RunnerResponse {
        RunnerResponse::RunnerCapabilities(RunnerCapabilitiesResponse {
            protocol_version: RUNNER_WIRE_VERSION,
            harnesses: vec!["pi".to_string()],
            features: RunnerFeatureFlags {
                command_discovery: true,
                model_discovery: true,
                fork: true,
                extension_ui: true,
            },
        })
    }

    /// Get available commands.
    async fn agent_get_commands(&self, req: AgentGetCommandsRequest) -> RunnerResponse {
        debug!("agent_get_commands: session_id={}", req.session_id);

        match self.pi_manager.get_commands(&req.session_id).await {
            Ok(commands) => {
                // Parse JSON response to typed AgentCommandInfo vec
                let commands_vec: Vec<AgentCommandInfo> = commands
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| serde_json::from_value(v.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default();
                RunnerResponse::AgentCommands(AgentCommandsResponse {
                    session_id: req.session_id,
                    commands: commands_vec,
                })
            }
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to get commands: {}", e),
            ),
        }
    }

    /// Execute a bash command.
    async fn pi_bash(&self, req: PiBashRequest) -> RunnerResponse {
        debug!(
            "pi_bash: session_id={}, command={}",
            req.session_id, req.command
        );

        match self.pi_manager.bash(&req.session_id, &req.command).await {
            Ok(result) => {
                let output = result
                    .get("output")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let exit_code = result
                    .get("exit_code")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32;
                let cancelled = result
                    .get("cancelled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let truncated = result
                    .get("truncated")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let full_output_path = result
                    .get("full_output_path")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                RunnerResponse::PiBashResult(PiBashResultResponse {
                    session_id: req.session_id,
                    output,
                    exit_code,
                    cancelled,
                    truncated,
                    full_output_path,
                })
            }
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to execute bash: {}", e),
            ),
        }
    }

    /// Abort a running bash command.
    async fn pi_abort_bash(&self, req: PiAbortBashRequest) -> RunnerResponse {
        debug!("pi_abort_bash: session_id={}", req.session_id);

        match self.pi_manager.abort_bash(&req.session_id).await {
            Ok(()) => RunnerResponse::PiCommandAck {
                session_id: req.session_id,
            },
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to abort bash: {}", e),
            ),
        }
    }

    /// Respond to an extension UI prompt.
    async fn pi_extension_ui_response(&self, req: PiExtensionUiResponseRequest) -> RunnerResponse {
        debug!(
            "pi_extension_ui_response: session_id={}, id={}",
            req.session_id, req.id
        );

        match self
            .pi_manager
            .extension_ui_response(
                &req.session_id,
                &req.id,
                req.value.as_deref(),
                req.confirmed,
                req.cancelled,
            )
            .await
        {
            Ok(()) => RunnerResponse::PiCommandAck {
                session_id: req.session_id,
            },
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to send extension UI response: {}", e),
            ),
        }
    }

    /// Handle Pi subscription streaming.
    /// Subscribes to the PiSessionManager's per-subscriber channel and streams events.
    ///
    /// Each subscriber gets its own unbounded mpsc channel, guaranteeing zero
    /// event loss. Unlike the old broadcast approach, a slow subscriber never
    /// causes events to be dropped.
    fn serialize_response_line(
        resp: &RunnerResponse,
    ) -> std::result::Result<String, std::io::Error> {
        let encoded = encode_json_frame(resp)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        String::from_utf8(encoded)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }

    async fn handle_pi_subscribe<W>(
        &self,
        session_id: &str,
        resume: Option<&PiResumeCursor>,
        writer: &mut W,
    ) -> Result<(), std::io::Error>
    where
        W: AsyncWrite + Unpin + ?Sized,
    {
        info!("handle_pi_subscribe: session_id={}", session_id);

        // Subscribe to the session's event stream
        let mut attachment = match self.pi_manager.subscribe(session_id, resume).await {
            Ok(attachment) => attachment,
            Err(e) => {
                // Session doesn't exist - send error and end
                let resp = error_response(
                    ErrorCode::PiSessionNotFound,
                    format!("Failed to subscribe: {}", e),
                );
                let line = Self::serialize_response_line(&resp)?;
                writer.write_all(line.as_bytes()).await?;
                return Ok(());
            }
        };

        // Send subscription confirmation
        let resp = RunnerResponse::PiSubscribed(PiSubscribedResponse {
            session_id: session_id.to_string(),
            stream_id: attachment.stream_id.clone(),
            attachment: attachment.status,
            replay_floor: attachment.replay_floor,
            current_seq: attachment.current_seq,
            delivered_through: attachment.delivered_through,
        });
        let line = Self::serialize_response_line(&resp)?;
        writer.write_all(line.as_bytes()).await?;

        for event in attachment.replay.drain(..) {
            let line = Self::serialize_response_line(&RunnerResponse::PiEvent(Box::new(event)))?;
            writer.write_all(line.as_bytes()).await?;
        }

        // Stream events until the session closes or client disconnects.
        // The channel is unbounded per-subscriber so events are never dropped.
        // When the session ends, the sender side is dropped and recv() returns None.
        while let Some(event_wrapper) = attachment.receiver.recv().await {
            let resp = RunnerResponse::PiEvent(Box::new(event_wrapper));
            let line = match Self::serialize_response_line(&resp) {
                Ok(line) => line,
                Err(err) => {
                    error!("Failed to serialize Pi event response: {}", err);
                    break;
                }
            };
            if writer.write_all(line.as_bytes()).await.is_err() {
                // Client disconnected
                debug!("Pi subscription client disconnected: {}", session_id);
                break;
            }
        }

        // Send subscription end notification
        let end_resp = RunnerResponse::PiSubscriptionEnd(PiSubscriptionEndResponse {
            session_id: session_id.to_string(),
            reason: "session_closed".to_string(),
        });
        if let Ok(line) = Self::serialize_response_line(&end_resp) {
            let _ = writer.write_all(line.as_bytes()).await;
        }

        Ok(())
    }

    /// Handle a client connection over any established bidirectional stream.
    async fn handle_connection<S>(&self, stream: S)
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (reader, mut writer) = tokio::io::split(stream);
        let mut reader = BufReader::new(reader);

        loop {
            match read_json_frame::<_, RunnerRequest>(&mut reader).await {
                Ok(None) => {
                    debug!("Client disconnected");
                    break;
                }
                Err(error) => {
                    let resp = error_response(
                        ErrorCode::InvalidRequest,
                        format!("Invalid runner frame: {error}"),
                    );
                    if let Ok(line) = Self::serialize_response_line(&resp) {
                        let _ = writer.write_all(line.as_bytes()).await;
                    }
                    break;
                }
                Ok(Some(req)) => {
                    debug!("Received request: {:?}", req);

                    // Handle PiSubscribe specially since it streams
                    if let RunnerRequest::PiSubscribe(ref sub_req) = req {
                        let session_id = sub_req.session_id.clone();
                        if let Err(e) = self
                            .handle_pi_subscribe(&session_id, sub_req.resume.as_ref(), &mut writer)
                            .await
                        {
                            error!("Failed to handle Pi subscription: {}", e);
                            break;
                        }
                        // After subscription ends, continue the connection loop
                        continue;
                    }

                    // Handle SubscribeStdout specially since it streams
                    if let RunnerRequest::SubscribeStdout(ref sub_req) = req {
                        let process_id = sub_req.id.clone();
                        match self.get_stdout_receiver(&process_id).await {
                            Ok((mut rx, buffered_lines)) => {
                                // Send subscription confirmation
                                let resp =
                                    RunnerResponse::StdoutSubscribed(StdoutSubscribedResponse {
                                        id: process_id.clone(),
                                    });
                                let line = match Self::serialize_response_line(&resp) {
                                    Ok(line) => line,
                                    Err(err) => {
                                        error!(
                                            "Failed to serialize stdout subscribe response: {}",
                                            err
                                        );
                                        break;
                                    }
                                };
                                if writer.write_all(line.as_bytes()).await.is_err() {
                                    break;
                                }

                                // Send any buffered lines first
                                for buffered_line in buffered_lines {
                                    let resp = RunnerResponse::StdoutLine(StdoutLineResponse {
                                        id: process_id.clone(),
                                        line: buffered_line,
                                    });
                                    let line = match Self::serialize_response_line(&resp) {
                                        Ok(line) => line,
                                        Err(err) => {
                                            error!(
                                                "Failed to serialize buffered stdout line: {}",
                                                err
                                            );
                                            break;
                                        }
                                    };
                                    if writer.write_all(line.as_bytes()).await.is_err() {
                                        break;
                                    }
                                }

                                // Stream new lines as they arrive
                                loop {
                                    match rx.recv().await {
                                        Ok(StdoutEvent::Line(stdout_line)) => {
                                            let resp =
                                                RunnerResponse::StdoutLine(StdoutLineResponse {
                                                    id: process_id.clone(),
                                                    line: stdout_line,
                                                });
                                            let line = match Self::serialize_response_line(&resp) {
                                                Ok(line) => line,
                                                Err(err) => {
                                                    error!(
                                                        "Failed to serialize stdout line event: {}",
                                                        err
                                                    );
                                                    break;
                                                }
                                            };
                                            if writer.write_all(line.as_bytes()).await.is_err() {
                                                break;
                                            }
                                        }
                                        Ok(StdoutEvent::Closed { exit_code }) => {
                                            let resp =
                                                RunnerResponse::StdoutEnd(StdoutEndResponse {
                                                    id: process_id.clone(),
                                                    exit_code,
                                                });
                                            if let Ok(line) = Self::serialize_response_line(&resp) {
                                                let _ = writer.write_all(line.as_bytes()).await;
                                            }
                                            break;
                                        }
                                        Err(broadcast::error::RecvError::Lagged(n)) => {
                                            warn!(
                                                "Stdout subscription lagged, missed {} events",
                                                n
                                            );
                                            // Continue receiving
                                        }
                                        Err(broadcast::error::RecvError::Closed) => {
                                            let resp =
                                                RunnerResponse::StdoutEnd(StdoutEndResponse {
                                                    id: process_id.clone(),
                                                    exit_code: None,
                                                });
                                            if let Ok(line) = Self::serialize_response_line(&resp) {
                                                let _ = writer.write_all(line.as_bytes()).await;
                                            }
                                            break;
                                        }
                                    }
                                }
                                // After subscription ends, continue the connection loop
                                // (client can send more requests)
                                continue;
                            }
                            Err(resp) => {
                                let line = match Self::serialize_response_line(&resp) {
                                    Ok(line) => line,
                                    Err(err) => {
                                        error!(
                                            "Failed to serialize stdout subscription error: {}",
                                            err
                                        );
                                        break;
                                    }
                                };
                                if writer.write_all(line.as_bytes()).await.is_err() {
                                    break;
                                }
                                continue;
                            }
                        }
                    }

                    let timeout = Self::request_timeout(&req);
                    let kind = Self::request_kind(&req);
                    let resp = match tokio::time::timeout(timeout, self.handle_request(req)).await {
                        Ok(resp) => resp,
                        Err(_) => {
                            error!(
                                "runner request timed out: request={} timeout_secs={}",
                                kind,
                                timeout.as_secs()
                            );
                            error_response(
                                ErrorCode::Internal,
                                format!(
                                    "runner request '{}' timed out after {}s",
                                    kind,
                                    timeout.as_secs()
                                ),
                            )
                        }
                    };
                    let line = match Self::serialize_response_line(&resp) {
                        Ok(line) => line,
                        Err(err) => {
                            error!("Failed to serialize response: {}", err);
                            break;
                        }
                    };
                    if let Err(e) = writer.write_all(line.as_bytes()).await {
                        error!("Failed to write response: {}", e);
                        break;
                    }
                }
            }
        }
    }

    async fn serve_listener<L>(&self, listener: &L)
    where
        L: RunnerListener,
    {
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok(stream) => {
                            debug!("New client connection on {}", listener.endpoint_description());
                            let runner = Runner {
                                state: Arc::clone(&self.state),
                                shutdown_tx: self.shutdown_tx.clone(),
                                sandbox_config: self.sandbox_config.clone(),
                                binaries: self.binaries.clone(),
                                user_config: self.user_config.clone(),
                                pi_manager: Arc::clone(&self.pi_manager),
                                jsonl_ingest: self.jsonl_ingest.clone(),
                            };
                            tokio::spawn(async move {
                                runner.handle_connection(stream).await;
                            });
                        }
                        Err(error) => {
                            error!("Accept error on {}: {}", listener.endpoint_description(), error);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutting down listener {}", listener.endpoint_description());
                    break;
                }
            }
        }
    }

    /// Serve any authenticated runner transport until shutdown.
    pub async fn run_transport<L>(&self, listener: &L) -> Result<()>
    where
        L: RunnerListener,
    {
        info!("Runner listening on {}", listener.endpoint_description());
        sd_notify_ready();

        // Container-entrypoint correctness: SIGTERM/SIGINT trigger the same
        // graceful shutdown as an explicit request, so `podman stop` cleans
        // up sessions instead of timing out into SIGKILL.
        {
            let shutdown = self.shutdown_tx.clone();
            match (
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()),
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()),
            ) {
                (Ok(mut term), Ok(mut int)) => {
                    tokio::spawn(async move {
                        tokio::select! {
                            _ = term.recv() => info!("Received SIGTERM, shutting down"),
                            _ = int.recv() => info!("Received SIGINT, shutting down"),
                        }
                        let _ = shutdown.send(());
                    });
                }
                (term, int) => {
                    warn!(
                        "Failed to install signal handlers (term: {:?}, int: {:?})",
                        term.err(),
                        int.err()
                    );
                }
            }
        }

        // One-time index bootstrap: session lookups are O(1) via the oqto-log
        // session index; build it in the background if this home predates it.
        if let Some(home) = dirs::home_dir()
            && !oqto_history::oqto_log::index::index_exists(&home)
        {
            tokio::spawn(async move {
                let started = std::time::Instant::now();
                match oqto_history::oqto_log::index::rebuild(&home).await {
                    Ok(count) => info!(
                        "oqto-log session index built: {} sessions in {:?}",
                        count,
                        started.elapsed()
                    ),
                    Err(err) => warn!("oqto-log session index rebuild failed: {err:#}"),
                }
            });
        }
        self.serve_listener(listener).await;
        self.cleanup_managed_processes().await;
        Ok(())
    }

    async fn cleanup_managed_processes(&self) {
        let mut state = self.state.write().await;
        for (id, mut process) in state.processes.drain() {
            if process.is_running() {
                info!("Killing process '{}' on shutdown", id);
                let _ = process.child.kill().await;
            }
        }
    }

    /// Run the daemon on a Unix listener inherited via LISTEN_FDS
    /// (systemd/quadlet socket activation). The activation manager owns the
    /// socket file: no permission changes, no unlink on exit.
    pub async fn run_inherited(&self, listener: std::os::unix::net::UnixListener) -> Result<()> {
        listener
            .set_nonblocking(true)
            .context("setting inherited listener non-blocking")?;
        let listener = UnixListener::from_std(listener)
            .context("adopting inherited listener into the runtime")?;
        let listener = UnixRunnerListener::new(listener, "<inherited-fd-3>");
        self.run_transport(&listener).await
    }

    /// Run the daemon, listening on the given socket path.
    pub async fn run(&self, socket_path: &PathBuf) -> Result<()> {
        // Ensure the socket directory exists.
        // Normally created by oqto-usermgr, but we create it ourselves if missing
        // (e.g. after systemd restart). The parent dir has SGID group oqto, mode 2770,
        // so our new dir inherits group oqto -- which lets the oqto backend traverse it.
        // Socket permission policy. Default: SGID group dir + group socket so
        // the backend (same group) can connect. World mode (auto-userns
        // container placement): uid/group checks cannot cross the user
        // namespace, so the socket is world-connectable; authorization is the
        // backend-private host directory that contains it.
        let world_mode = std::env::var("OQTO_SOCKET_MODE").as_deref() == Ok("world");
        let (dir_mode, socket_mode) = if world_mode {
            (0o711, 0o666)
        } else {
            (0o2770, 0o770)
        };
        if let Some(parent) = socket_path.parent() {
            if !parent.exists() {
                info!("Socket directory {:?} does not exist, creating", parent);
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating socket directory {:?}", parent))?;
            }
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(dir_mode))
                .with_context(|| format!("chmod socket dir {:?}", parent))?;
        }

        // Remove existing socket file
        let _ = tokio::fs::remove_file(socket_path).await;

        // Bind
        let listener = UnixListener::bind(socket_path)
            .with_context(|| format!("binding to {:?}", socket_path))?;

        // Unix sockets require write permission for connect().
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(socket_mode))
            .with_context(|| format!("setting socket permissions on {:?}", socket_path))?;

        let listener = UnixRunnerListener::new(listener, socket_path);
        self.run_transport(&listener).await?;

        // Remove socket file
        let _ = tokio::fs::remove_file(socket_path).await;

        info!("Runner stopped");
        Ok(())
    }
}

fn error_response(code: ErrorCode, message: impl Into<String>) -> RunnerResponse {
    RunnerResponse::Error(ErrorResponse {
        code,
        message: message.into(),
    })
}

/// Notify systemd that the service is ready (sd_notify READY=1).
/// No-op if $NOTIFY_SOCKET is not set (i.e., not running under systemd Type=notify).
fn sd_notify_ready() {
    let Some(addr) = std::env::var_os("NOTIFY_SOCKET") else {
        return;
    };
    if let Ok(sock) = std::os::unix::net::UnixDatagram::unbound() {
        let _ = sock.send_to(b"READY=1", &addr);
    }
}

// ============================================================================
// TRX helpers
// ============================================================================

/// Default issue ID prefix when auto-initializing a new trx store.
/// Matches the trx CLI default; users can pre-init with a custom prefix
/// via `trx init --prefix X` before any oqto-side trx call.
const TRX_DEFAULT_PREFIX: &str = "trx";

/// Open (init-if-missing) the workspace `.mmry/mmry.jsonl` memory ledger.
fn open_workspace_memory(
    workspace_path: &std::path::Path,
) -> anyhow::Result<mmry_core::memory_file::MemoryFile> {
    if workspace_path.as_os_str().is_empty() {
        anyhow::bail!("empty workspace path");
    }
    let file = mmry_core::memory_file::MemoryFile::open_workspace(workspace_path);
    file.init(false)
        .map_err(|e| anyhow::anyhow!("init memory file: {e}"))?;
    Ok(file)
}

fn parse_wire_memory_type(memory_type: Option<&str>) -> mmry_core::memory::MemoryType {
    match memory_type.unwrap_or("semantic").to_lowercase().as_str() {
        "episodic" => mmry_core::memory::MemoryType::Episodic,
        "procedural" => mmry_core::memory::MemoryType::Procedural,
        _ => mmry_core::memory::MemoryType::Semantic,
    }
}

fn apply_memory_metadata(
    event: &mut mmry_core::memory_file::MemoryEvent,
    category: Option<String>,
    importance: Option<u8>,
) {
    if let Some(category) = category {
        event.metadata["category"] = serde_json::Value::String(category);
    }
    if let Some(importance) = importance {
        event.metadata["importance"] = serde_json::Value::Number(importance.into());
    }
}

fn memory_entry_to_wire(
    entry: mmry_core::memory_file::MemoryEntry,
    score: Option<f64>,
) -> MemoryEntry {
    let category = entry
        .metadata
        .get("category")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let importance = entry
        .metadata
        .get("importance")
        .and_then(serde_json::Value::as_u64)
        .map(|v| v as u8);
    MemoryEntry {
        id: entry.memory_id,
        content: entry.content,
        category,
        importance,
        memory_type: format!("{:?}", entry.memory_type).to_lowercase(),
        tags: entry.tags,
        metadata: entry.metadata,
        created_at: entry.created_at.to_rfc3339(),
        updated_at: entry.updated_at.to_rfc3339(),
        score,
    }
}

fn find_wire_memory(
    file: &mmry_core::memory_file::MemoryFile,
    memory_id: &str,
) -> anyhow::Result<MemoryEntry> {
    file.active_memories()?
        .into_iter()
        .find(|m| m.memory_id == memory_id)
        .map(|m| memory_entry_to_wire(m, None))
        .ok_or_else(|| anyhow::anyhow!("memory {memory_id} not found after write"))
}

/// Open the trx store at `root`, auto-initializing when not yet initialized.
fn open_or_init_trx_store(root: &std::path::Path) -> anyhow::Result<trx_core::Store> {
    let store_root = find_trx_root(root).unwrap_or_else(|| root.to_path_buf());
    match trx_core::Store::open_at(store_root.clone()) {
        Ok(store) => Ok(store),
        Err(trx_core::Error::NotInitialized) => {
            let current = std::env::current_dir().context("reading current dir")?;
            std::env::set_current_dir(&store_root)
                .with_context(|| format!("setting cwd to {}", store_root.display()))?;
            let init_result = trx_core::Store::init(TRX_DEFAULT_PREFIX);
            let _ = std::env::set_current_dir(current);
            init_result.map_err(|e| anyhow::anyhow!("trx init failed: {e}"))
        }
        Err(e) => Err(anyhow::anyhow!("trx open failed: {e}")),
    }
}

fn find_trx_root(start: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".trx").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Convert a trx-core `Issue` into the wire-format `TrxIssueData` that
/// crosses the runner socket back to oqto.
fn trx_issue_to_data(issue: &trx_core::Issue) -> TrxIssueData {
    let parent_id = issue
        .dependencies
        .iter()
        .find(|d| {
            matches!(d.dep_type, trx_core::DependencyType::ParentChild)
                || (matches!(d.dep_type, trx_core::DependencyType::Blocks)
                    && issue.id.starts_with(&format!("{}.", d.depends_on_id)))
        })
        .map(|d| d.depends_on_id.clone());

    let blocked_by: Vec<String> = issue
        .dependencies
        .iter()
        .filter(|d| matches!(d.dep_type, trx_core::DependencyType::Blocks))
        .map(|d| d.depends_on_id.clone())
        .collect();

    TrxIssueData {
        id: issue.id.clone(),
        title: issue.title.clone(),
        description: issue.description.clone(),
        status: issue.status.to_string(),
        priority: i32::from(issue.priority),
        issue_type: issue.issue_type.to_string(),
        created_at: issue.created_at.to_rfc3339(),
        updated_at: issue.updated_at.to_rfc3339(),
        closed_at: issue.closed_at.map(|t| t.to_rfc3339()),
        parent_id,
        blocked_by,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fork_copy_validation_matches_pi_position_before_semantics() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let parent = temp.path().join("parent.jsonl");
        let child = temp.path().join("child.jsonl");
        std::fs::write(
            &parent,
            concat!(
                "{\"type\":\"session\",\"id\":\"parent\"}\n",
                "{\"type\":\"message\",\"id\":\"u1\",\"parentId\":null,\"message\":{\"role\":\"user\"}}\n",
                "{\"type\":\"message\",\"id\":\"a1\",\"parentId\":\"u1\",\"message\":{\"role\":\"assistant\"}}\n",
                "{\"type\":\"message\",\"id\":\"u2\",\"parentId\":\"a1\",\"message\":{\"role\":\"user\"}}\n",
                "{\"type\":\"message\",\"id\":\"a2\",\"parentId\":\"u2\",\"message\":{\"role\":\"assistant\"}}\n",
            ),
        )?;
        let child_header = serde_json::json!({
            "type": "session",
            "id": "child",
            "parentSession": parent,
        });
        std::fs::write(
            &child,
            format!(
                "{}\n{}{}",
                child_header,
                "{\"type\":\"message\",\"id\":\"u1\",\"parentId\":null,\"message\":{\"role\":\"user\"}}\n",
                "{\"type\":\"message\",\"id\":\"a1\",\"parentId\":\"u1\",\"message\":{\"role\":\"assistant\"}}\n",
            ),
        )?;
        validate_fork_copy(&parent, &child, "u2")?;

        std::fs::write(
            &child,
            format!(
                "{}\n{}{}{}",
                child_header,
                "{\"type\":\"message\",\"id\":\"u1\",\"parentId\":null,\"message\":{\"role\":\"user\"}}\n",
                "{\"type\":\"message\",\"id\":\"a1\",\"parentId\":\"u1\",\"message\":{\"role\":\"assistant\"}}\n",
                "{\"type\":\"message\",\"id\":\"u2\",\"parentId\":\"a1\",\"message\":{\"role\":\"user\"}}\n",
            ),
        )?;
        assert!(validate_fork_copy(&parent, &child, "u2").is_err());
        Ok(())
    }

    fn proto(id: &str, role: &str, created_at: i64) -> ChatMessageProto {
        ChatMessageProto {
            id: id.to_string(),
            session_id: "sess-1".to_string(),
            role: role.to_string(),
            created_at,
            completed_at: Some(created_at),
            parent_id: None,
            model_id: None,
            provider_id: None,
            agent: None,
            summary_title: None,
            tokens_input: None,
            tokens_output: None,
            tokens_reasoning: None,
            cost: None,
            client_id: None,
            parts: vec![ChatMessagePartProto {
                id: format!("p-{id}"),
                part_type: "text".to_string(),
                text: Some(id.to_string()),
                text_html: None,
                tool_name: None,
                tool_call_id: None,
                tool_input: None,
                tool_output: None,
                tool_status: None,
                tool_title: None,
            }],
        }
    }

    #[test]
    fn workspace_memory_round_trips_through_ledger_helpers() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let ws = temp.path();

        let file = open_workspace_memory(ws)?;
        let ctx = mmry_core::agent_ctx::AgentCtx::from_env();
        let mut event = mmry_core::memory_file::MemoryEvent::add(
            "runner memory".to_string(),
            parse_wire_memory_type(Some("semantic")),
            vec!["backend".to_string()],
            &ctx,
        );
        apply_memory_metadata(&mut event, Some("architecture".to_string()), Some(7));
        file.append(&event)?;
        let memory_id = event.memory_id.clone();

        // Reopen to prove durability across handles.
        let reopened = open_workspace_memory(ws)?;
        let wire = find_wire_memory(&reopened, &memory_id)?;
        assert_eq!(wire.content, "runner memory");
        assert_eq!(wire.category.as_deref(), Some("architecture"));
        assert_eq!(wire.importance, Some(7));
        assert_eq!(wire.memory_type, "semantic");
        assert_eq!(wire.tags, vec!["backend".to_string()]);

        // Search finds it.
        let hits = reopened.search("runner", 10)?;
        assert!(hits.iter().any(|h| h.memory.memory_id == memory_id));

        // Deprecation removes it from the active set.
        reopened.append(&mmry_core::memory_file::MemoryEvent::deprecate(
            memory_id.clone(),
            &ctx,
        ))?;
        let after = open_workspace_memory(ws)?;
        assert!(
            after
                .active_memories()?
                .iter()
                .all(|m| m.memory_id != memory_id)
        );
        Ok(())
    }

    #[test]
    fn repair_workspace_chat_history_has_extended_timeout_budget() {
        let repair = RunnerRequest::RepairWorkspaceChatHistory(RepairWorkspaceChatHistoryRequest {
            limit: Some(500),
            workspace: None,
        });

        assert_eq!(Runner::request_timeout(&repair).as_secs(), 120);
        assert_eq!(Runner::request_timeout(&RunnerRequest::Ping).as_secs(), 10);
    }

    #[test]
    fn pi_jsonl_record_reader_preserves_entry_ids_and_parents() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("session.jsonl");
        std::fs::write(
            &path,
            r#"{"type":"session","version":3,"id":"pi-session","timestamp":"2026-01-01T00:00:00Z","cwd":"/tmp"}
{"type":"message","id":"aaaa1111","parentId":null,"timestamp":"2026-01-01T00:00:01Z","message":{"role":"user","content":"hello","timestamp":1770000000000}}
{"type":"message","id":"bbbb2222","parentId":"aaaa1111","timestamp":"2026-01-01T00:00:02Z","message":{"role":"assistant","content":[{"type":"text","text":"hi"}],"timestamp":1770000001000}}
"#,
        )
        .expect("write jsonl");

        let records = read_jsonl_message_records(&path);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].source_entry_id, "aaaa1111");
        assert_eq!(records[0].parent_source_entry_id, None);
        assert_eq!(records[0].source_sequence, 1);
        assert_eq!(records[1].source_entry_id, "bbbb2222");
        assert_eq!(
            records[1].parent_source_entry_id.as_deref(),
            Some("aaaa1111")
        );
        assert_eq!(records[1].source_sequence, 2);
    }

    #[test]
    fn persistence_contracts_authoritative_path_uses_only_persisted_history() {
        let authoritative = vec![
            proto("hist-u1", "user", 1_000),
            proto("hist-a1", "assistant", 2_000),
        ];

        let (selected, source) =
            select_authoritative_workspace_chat_messages(Some(authoritative.clone()));

        assert_eq!(source, "oqto-log");
        assert_eq!(selected.len(), authoritative.len());
        assert_eq!(selected[0].id, "hist-u1");
        assert_eq!(selected[1].id, "hist-a1");
    }

    #[test]
    fn persistence_contracts_authoritative_path_keeps_oqto_log_even_when_jsonl_is_richer() {
        let projected = vec![proto("hist-a1", "assistant", 2_000)];
        let jsonl = vec![
            proto("jsonl-u1", "user", 1_000),
            proto("jsonl-a1", "assistant", 2_000),
        ];

        let (selected, source) =
            select_richest_authoritative_messages(Some(projected), Some(jsonl));

        assert_eq!(source, "oqto-log");
        let selected = selected.expect("selected messages");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "hist-a1");
    }

    #[test]
    fn persistence_contracts_authoritative_path_keeps_oqto_log_when_counts_match() {
        let projected = vec![
            proto("hist-u1", "user", 1_000),
            proto("hist-a1", "assistant", 2_000),
        ];
        let jsonl = vec![
            proto("jsonl-u1", "user", 1_000),
            proto("jsonl-a1", "assistant", 2_000),
        ];

        let (selected, source) =
            select_richest_authoritative_messages(Some(projected), Some(jsonl));

        assert_eq!(source, "oqto-log");
        assert_eq!(selected.expect("selected messages")[0].id, "hist-u1");
    }

    #[test]
    fn persistence_contracts_live_path_uses_only_live_buffer() {
        let live = vec![
            proto("live-u1", "user", 1_000),
            proto("live-a1", "assistant", 2_000),
        ];

        let (selected, source) = select_live_workspace_chat_messages(Some(live.clone()));

        assert_eq!(source, "live_buffer");
        assert_eq!(selected.len(), live.len());
        assert_eq!(selected[0].id, "live-u1");
        assert_eq!(selected[1].id, "live-a1");
    }

    #[test]
    fn persistence_contracts_live_path_empty_when_buffer_missing() {
        let (selected, source) = select_live_workspace_chat_messages(None);

        assert_eq!(source, "live_buffer");
        assert!(selected.is_empty());
    }

    #[test]
    fn ttyd_is_always_started_with_a_credential() {
        let args = build_ttyd_args(7681, "/workspace", "s3cr3t");

        let cred_idx = args
            .iter()
            .position(|a| a == "--credential")
            .expect("ttyd must be started with --credential");
        assert_eq!(args[cred_idx + 1], format!("{}:s3cr3t", TERMINAL_USERNAME));
        assert!(
            args.iter().any(|a| a == "--check-origin"),
            "ttyd must reject cross-origin upgrades"
        );
    }

    #[test]
    fn ttyd_credentials_are_unguessable_and_unique() {
        let a = generate_terminal_credential();
        let b = generate_terminal_credential();

        assert_ne!(a, b, "credentials must not repeat across sessions");
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn ttyd_still_binds_loopback_only() {
        let args = build_ttyd_args(7681, "/workspace", "pw");
        let idx = args.iter().position(|a| a == "--interface").unwrap();
        assert_eq!(args[idx + 1], "127.0.0.1");
    }
}
