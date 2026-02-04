//! Pi workspace sessions service.
//!
//! Manages one Pi process per workspace session (per user), with idle cleanup.

use anyhow::{Context, Result};
use base64::Engine;
use chrono::{DateTime, TimeZone, Utc};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::local::LinuxUsersConfig;
use crate::main_chat::{MainChatPiServiceConfig, PiRuntimeMode, UserPiSession};
use crate::pi::{ContainerPiRuntime, LocalPiRuntime, PiRuntime, PiSpawnConfig, RunnerPiRuntime};
use crate::runner::client::RunnerClient;

/// How often to run the cleanup task (1 minute).
const CLEANUP_INTERVAL_SECS: u64 = 60;

/// Key for workspace sessions map: (user_id, workspace_path, session_id).
type WorkspaceSessionKey = (String, String, String);

/// A Pi session file on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionFile {
    /// Session ID (UUID from filename)
    pub id: String,
    /// Session start timestamp (ISO 8601)
    pub started_at: String,
    /// File size in bytes
    pub size: u64,
    /// Last modification time (Unix timestamp ms)
    pub modified_at: i64,
    /// Title (derived from first user message, or None)
    pub title: Option<String>,
    /// Parent session ID (if this session was spawned as a child)
    pub parent_id: Option<String>,
    /// Number of messages in session
    pub message_count: usize,
}

/// A message from a Pi session file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionMessage {
    /// Message ID
    pub id: String,
    /// Role: user, assistant, system
    pub role: String,
    /// Content (text or structured)
    pub content: Value,
    /// Tool call ID (toolResult messages only)
    #[serde(rename = "toolCallId", skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool name (toolResult messages only)
    #[serde(rename = "toolName", skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Tool error flag (toolResult messages only)
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    /// Timestamp (Unix ms)
    pub timestamp: i64,
    /// Usage stats (for assistant messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
}

/// Summary info for a workspace Pi session.
#[derive(Debug, Clone)]
pub struct WorkspacePiSessionSummary {
    pub id: String,
    pub title: Option<String>,
    pub readable_id: Option<String>,
    pub parent_id: Option<String>,
    pub workspace_path: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub version: Option<String>,
    pub source_path: Option<String>,
}

/// Service for managing Pi sessions for workspace chats.
pub struct WorkspacePiService {
    /// Configuration.
    config: MainChatPiServiceConfig,
    /// Active sessions keyed by (user_id, workspace_path, session_id).
    sessions: RwLock<HashMap<WorkspaceSessionKey, Arc<UserPiSession>>>,
    /// Keys currently being created (to prevent duplicate spawns from concurrent requests).
    creating: Mutex<HashSet<WorkspaceSessionKey>>,
    /// Idle timeout in seconds (sessions idle longer than this may be cleaned up).
    idle_timeout_secs: u64,
    /// Linux user isolation configuration (multi-user mode).
    linux_users: Option<LinuxUsersConfig>,
}

impl WorkspacePiService {
    pub fn new(config: MainChatPiServiceConfig, linux_users: Option<LinuxUsersConfig>) -> Self {
        info!(
            "WorkspacePiService initialized with runtime mode: {}",
            config.runtime_mode
        );

        let idle_timeout_secs = config.idle_timeout_secs;
        Self {
            config,
            sessions: RwLock::new(HashMap::new()),
            creating: Mutex::new(HashSet::new()),
            idle_timeout_secs,
            linux_users,
        }
    }

    /// Start the background cleanup task for idle sessions.
    pub fn start_cleanup_task(self: &Arc<Self>) {
        let service = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(CLEANUP_INTERVAL_SECS));
            loop {
                interval.tick().await;
                service.cleanup_idle_sessions().await;
            }
        });
    }

    pub async fn remove_session(&self, user_id: &str, work_dir: &Path, session_id: &str) -> bool {
        let key = (
            user_id.to_string(),
            work_dir.to_string_lossy().to_string(),
            session_id.to_string(),
        );
        let mut sessions = self.sessions.write().await;
        sessions.remove(&key).is_some()
    }

    /// Clean up idle sessions that are not streaming.
    async fn cleanup_idle_sessions(&self) {
        let idle_threshold = Duration::from_secs(self.idle_timeout_secs);
        let mut to_remove: Vec<WorkspaceSessionKey> = Vec::new();

        {
            let sessions = self.sessions.read().await;
            for (key, session) in sessions.iter() {
                let is_streaming = session.is_streaming().await;
                if is_streaming {
                    continue;
                }
                let elapsed = session.last_activity_elapsed().await;
                if elapsed > idle_threshold {
                    debug!(
                        "Workspace Pi session {:?} idle for {:?}, scheduling cleanup",
                        key, elapsed
                    );
                    to_remove.push(key.clone());
                }
            }
        }

        if to_remove.is_empty() {
            return;
        }

        let mut sessions = self.sessions.write().await;
        for key in to_remove {
            sessions.remove(&key);
        }
    }

    fn create_runtime_for_user(&self, user_id: &str) -> Arc<dyn PiRuntime> {
        match self.config.runtime_mode {
            PiRuntimeMode::Local => Arc::new(LocalPiRuntime::new()),
            PiRuntimeMode::Runner => {
                let client = if let Some(pattern) = &self.config.runner_socket_pattern {
                    // Use for_user_with_pattern which handles both {user} and {uid} placeholders
                    match RunnerClient::for_user_with_pattern(user_id, pattern) {
                        Ok(c) => c,
                        Err(e) => {
                            warn!("Failed to create runner client for user {}: {}", user_id, e);
                            RunnerClient::default()
                        }
                    }
                } else {
                    RunnerClient::default()
                };
                Arc::new(RunnerPiRuntime::new(client))
            }
            PiRuntimeMode::Container => Arc::new(ContainerPiRuntime::new()),
        }
    }

    /// Get the Pi agent directory for a working directory.
    fn get_pi_agent_dir(&self, user_id: &str) -> PathBuf {
        let home = if let Some(linux_users) = self.linux_users.as_ref() {
            match linux_users.get_home_dir(user_id) {
                Ok(Some(home)) => Some(home),
                Ok(None) => {
                    warn!("Linux user home not found for user {}", user_id);
                    None
                }
                Err(err) => {
                    warn!("Failed to resolve linux user home for {}: {}", user_id, err);
                    None
                }
            }
        } else {
            dirs::home_dir()
        };

        home.map(|home| home.join(".pi").join("agent"))
            .unwrap_or_else(|| PathBuf::from("/nonexistent/.pi/agent"))
    }

    /// Get the Pi sessions directory for a working directory.
    /// Pi stores sessions in ~/.pi/agent/sessions/--<cwd>--/
    fn get_pi_sessions_dir(&self, user_id: &str, work_dir: &Path) -> PathBuf {
        let escaped_path = work_dir
            .to_string_lossy()
            .trim_start_matches(&['/', '\\'][..])
            .replace('/', "-")
            .replace('\\', "-")
            .replace(':', "-");
        self.get_pi_agent_dir(user_id)
            .join("sessions")
            .join(format!("--{}--", escaped_path))
    }

    fn runner_client_for_user(&self, user_id: &str) -> Option<RunnerClient> {
        self.linux_users.as_ref()?;
        let pattern = self.config.runner_socket_pattern.as_deref()?;
        // Use for_user_with_pattern which handles both {user} and {uid} placeholders
        match RunnerClient::for_user_with_pattern(user_id, pattern) {
            Ok(c) if c.socket_path().exists() => Some(c),
            Ok(_) => None,
            Err(e) => {
                warn!("Failed to create runner client for user {}: {}", user_id, e);
                None
            }
        }
    }

    fn map_pi_role(role: &str) -> &str {
        match role {
            "user" => "user",
            "assistant" => "assistant",
            "tool" | "toolResult" => "toolResult",
            "system" => "custom",
            _ => "assistant",
        }
    }

    fn derive_provider(model: &str) -> &'static str {
        let lower = model.to_lowercase();
        if lower.contains("claude") || lower.contains("anthropic") {
            return "anthropic";
        }
        if lower.contains("gpt") || lower.contains("openai") || lower.contains("codex") {
            return "openai";
        }
        if lower.contains("gemini") || lower.contains("google") {
            return "google";
        }
        if lower.contains("llama") || lower.contains("meta") {
            return "meta";
        }
        "unknown"
    }

    fn build_pi_session_jsonl(
        session_id: &str,
        cwd: &str,
        created_at_ms: i64,
        title: Option<String>,
        model: Option<String>,
        messages: Vec<(String, serde_json::Value, i64)>,
    ) -> String {
        let mut lines = Vec::new();
        let header = serde_json::json!({
            "type": "session",
            "version": 3,
            "id": session_id,
            "timestamp": Utc.timestamp_millis_opt(created_at_ms)
                .single()
                .unwrap_or_else(Utc::now)
                .to_rfc3339(),
            "cwd": cwd,
        });
        lines.push(serde_json::to_string(&header).unwrap_or_else(|_| "{}".to_string()));

        let mut last_entry_id: Option<String> = None;
        if let Some(model) = model.clone() {
            let model_entry = serde_json::json!({
                "type": "model_change",
                "id": Uuid::new_v4().simple().to_string(),
                "parentId": last_entry_id,
                "timestamp": Utc.timestamp_millis_opt(created_at_ms)
                    .single()
                    .unwrap_or_else(Utc::now)
                    .to_rfc3339(),
                "provider": Self::derive_provider(&model),
                "modelId": model,
            });
            if let Ok(line) = serde_json::to_string(&model_entry) {
                last_entry_id = model_entry
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                lines.push(line);
            }
        }

        if let Some(title) = title {
            let info_entry = serde_json::json!({
                "type": "session_info",
                "id": Uuid::new_v4().simple().to_string(),
                "parentId": last_entry_id,
                "timestamp": Utc.timestamp_millis_opt(created_at_ms)
                    .single()
                    .unwrap_or_else(Utc::now)
                    .to_rfc3339(),
                "name": title,
            });
            if let Ok(line) = serde_json::to_string(&info_entry) {
                last_entry_id = info_entry
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                lines.push(line);
            }
        }

        for (role, content, timestamp_ms) in messages {
            let msg_entry = serde_json::json!({
                "type": "message",
                "id": Uuid::new_v4().simple().to_string(),
                "parentId": last_entry_id,
                "timestamp": Utc.timestamp_millis_opt(timestamp_ms)
                    .single()
                    .unwrap_or_else(Utc::now)
                    .to_rfc3339(),
                "message": {
                    "role": Self::map_pi_role(&role),
                    "content": content,
                    "timestamp": timestamp_ms,
                }
            });
            if let Ok(line) = serde_json::to_string(&msg_entry) {
                last_entry_id = msg_entry
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                lines.push(line);
            }
        }

        lines.join("\n") + "\n"
    }

    async fn write_session_file(
        &self,
        user_id: &str,
        sessions_dir: &Path,
        session_id: &str,
        content: String,
    ) -> Result<PathBuf> {
        let filename = format!("{}_{}.jsonl", Utc::now().timestamp_millis(), session_id);
        let path = sessions_dir.join(filename);

        if let Some(client) = self.runner_client_for_user(user_id) {
            let _ = client.create_directory(sessions_dir, true).await;
            client
                .write_file(path.clone(), content.as_bytes(), true)
                .await
                .context("writing session file via runner")?;
            return Ok(path);
        }

        if !sessions_dir.exists() {
            std::fs::create_dir_all(sessions_dir).context("creating sessions directory")?;
        }
        std::fs::write(&path, content).context("writing session file")?;
        Ok(path)
    }

    async fn rehydrate_session_from_hstry(
        &self,
        user_id: &str,
        work_dir: &Path,
        session_id: &str,
    ) -> Result<Option<PathBuf>> {
        let sessions_dir = self.get_pi_sessions_dir(user_id, work_dir);

        if let Some(client) = self.runner_client_for_user(user_id) {
            let resp = client
                .get_workspace_chat_messages(
                    work_dir.to_string_lossy().to_string(),
                    session_id,
                    None,
                )
                .await
                .context("runner get_workspace_chat_messages")?;
            if resp.messages.is_empty() {
                return Ok(None);
            }

            let created_at_ms = resp
                .messages
                .first()
                .map(|m| m.timestamp)
                .unwrap_or_else(|| Utc::now().timestamp_millis());
            let messages = resp
                .messages
                .into_iter()
                .map(|m| (m.role, m.content, m.timestamp))
                .collect();
            let jsonl = Self::build_pi_session_jsonl(
                session_id,
                &work_dir.to_string_lossy(),
                created_at_ms,
                None,
                None,
                messages,
            );
            let path = self
                .write_session_file(user_id, &sessions_dir, session_id, jsonl)
                .await?;
            return Ok(Some(path));
        }

        let Some(db_path) = crate::history::hstry_db_path() else {
            return Ok(None);
        };
        let pool = crate::history::repository::open_hstry_pool(&db_path).await?;

        let conv_row = sqlx::query(
            r#"
            SELECT id, external_id, title, created_at, model, workspace
            FROM conversations
            WHERE source_id = 'pi'
              AND (external_id = ? OR readable_id = ? OR id = ?)
              AND workspace = ?
            LIMIT 1
            "#,
        )
        .bind(session_id)
        .bind(session_id)
        .bind(session_id)
        .bind(work_dir.to_string_lossy().to_string())
        .fetch_optional(&pool)
        .await?;

        let conv_row = if conv_row.is_some() {
            conv_row
        } else {
            sqlx::query(
                r#"
                SELECT id, external_id, title, created_at, model, workspace
                FROM conversations
                WHERE source_id = 'pi' AND (external_id = ? OR readable_id = ? OR id = ?)
                LIMIT 1
                "#,
            )
            .bind(session_id)
            .bind(session_id)
            .bind(session_id)
            .fetch_optional(&pool)
            .await?
        };

        let Some(conv_row) = conv_row else {
            return Ok(None);
        };

        let conversation_id: String = conv_row.try_get("id")?;
        let title: Option<String> = conv_row.try_get("title").ok();
        let model: Option<String> = conv_row.try_get("model").ok();
        let workspace: Option<String> = conv_row.try_get("workspace").ok();
        let created_at: i64 = conv_row.try_get("created_at").unwrap_or_else(|_| 0);
        let created_at_ms = created_at * 1000;

        let rows = sqlx::query(
            r#"
            SELECT role, content, created_at, parts_json
            FROM messages
            WHERE conversation_id = ?
            ORDER BY idx
            "#,
        )
        .bind(&conversation_id)
        .fetch_all(&pool)
        .await?;

        let mut messages = Vec::with_capacity(rows.len());
        for row in rows {
            let role: String = row
                .try_get("role")
                .unwrap_or_else(|_| "assistant".to_string());
            let content_raw: String = row.try_get("content").unwrap_or_default();
            let created_at: Option<i64> = row.try_get("created_at").ok();
            let parts_json: Option<String> = row.try_get("parts_json").ok();

            let content = if let Some(parts_json) = parts_json.as_deref()
                && let Ok(v) = serde_json::from_str::<serde_json::Value>(parts_json)
                && v.is_array()
            {
                v
            } else {
                serde_json::json!([{ "type": "text", "text": content_raw }])
            };

            let timestamp_ms = created_at
                .map(|ts| ts * 1000)
                .unwrap_or_else(|| Utc::now().timestamp_millis());
            messages.push((role, content, timestamp_ms));
        }

        let cwd = workspace.unwrap_or_else(|| work_dir.to_string_lossy().to_string());
        let jsonl = Self::build_pi_session_jsonl(
            session_id,
            &cwd,
            if created_at_ms > 0 {
                created_at_ms
            } else {
                Utc::now().timestamp_millis()
            },
            title,
            model,
            messages,
        );

        let path = self
            .write_session_file(user_id, &sessions_dir, session_id, jsonl)
            .await?;
        Ok(Some(path))
    }

    async fn ensure_session_file(
        &self,
        user_id: &str,
        work_dir: &Path,
        session_id: &str,
    ) -> Result<PathBuf> {
        let sessions_dir = self.get_pi_sessions_dir(user_id, work_dir);
        if let Ok(existing) = self.find_session_file_anywhere(session_id) {
            return Ok(existing);
        }

        let header = serde_json::json!({
            "type": "session",
            "version": 3,
            "id": session_id,
            "timestamp": Utc::now().to_rfc3339(),
            "cwd": work_dir.to_string_lossy(),
        });
        let content = format!("{}\n", serde_json::to_string(&header)?);
        let filename = format!("{}_{}.jsonl", Utc::now().timestamp_millis(), session_id);
        let path = sessions_dir.join(filename);

        if let Some(client) = self.runner_client_for_user(user_id) {
            client
                .write_file(&path, content.as_bytes(), true)
                .await
                .context("writing session file via runner")?;
            return Ok(path);
        }

        if !sessions_dir.exists() {
            std::fs::create_dir_all(&sessions_dir).context("creating sessions directory")?;
        }
        std::fs::write(&path, content).context("writing session file")?;
        Ok(path)
    }

    /// Find a Pi session file by ID (.jsonl format).
    fn find_session_file(&self, sessions_dir: &Path, session_id: &str) -> Result<PathBuf> {
        if !sessions_dir.exists() {
            anyhow::bail!("Sessions directory not found");
        }

        let entries = std::fs::read_dir(sessions_dir).context("reading sessions directory")?;
        let mut best: Option<(i64, PathBuf)> = None;
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if filename.contains(session_id) {
                    let modified_at = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    match best {
                        Some((best_ts, _)) if modified_at <= best_ts => {}
                        _ => best = Some((modified_at, path)),
                    }
                }
            }
        }

        if let Some((_, path)) = best {
            return Ok(path);
        }

        anyhow::bail!("Session not found: {}", session_id)
    }

    fn find_session_file_anywhere(&self, session_id: &str) -> Result<PathBuf> {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let sessions_root = home.join(".pi").join("agent").join("sessions");
        if !sessions_root.exists() {
            anyhow::bail!("Sessions root not found");
        }

        let mut best: Option<(i64, PathBuf)> = None;
        let roots = std::fs::read_dir(&sessions_root)
            .with_context(|| format!("reading Pi sessions root: {:?}", sessions_root))?;
        for root in roots.filter_map(|e| e.ok()) {
            let root_path = root.path();
            if !root_path.is_dir() {
                continue;
            }
            let entries = match std::fs::read_dir(&root_path) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if filename.contains(session_id) {
                        let modified_at = entry
                            .metadata()
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_millis() as i64)
                            .unwrap_or(0);
                        match best {
                            Some((best_ts, _)) if modified_at <= best_ts => {}
                            _ => best = Some((modified_at, path)),
                        }
                    }
                }
            }
        }

        if let Some((_, path)) = best {
            return Ok(path);
        }

        anyhow::bail!("Session not found: {}", session_id)
    }

    /// Soft-delete a workspace Pi session by marking the JSONL header as deleted.
    pub async fn mark_session_deleted(
        &self,
        user_id: &str,
        work_dir: &Path,
        session_id: &str,
    ) -> Result<bool> {
        use std::io::{BufRead, BufReader};

        let sessions_dir = self.get_pi_sessions_dir(user_id, work_dir);
        if let Some(client) = self.runner_client_for_user(user_id) {
            let listing = client
                .list_directory(&sessions_dir, false)
                .await
                .context("listing session directory via runner")?;
            let session_name = listing
                .entries
                .iter()
                .find(|entry| {
                    !entry.is_dir
                        && entry.name.ends_with(".jsonl")
                        && entry.name.contains(session_id)
                })
                .map(|entry| entry.name.clone())
                .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;
            let session_path = sessions_dir.join(session_name);

            let content = client
                .read_file(&session_path, None, None)
                .await
                .context("reading session file via runner")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(content.content_base64)
                .context("decoding session file base64")?;
            let reader = BufReader::new(std::io::Cursor::new(bytes));
            let mut lines: Vec<String> = reader.lines().collect::<std::io::Result<_>>()?;
            if lines.is_empty() {
                anyhow::bail!("Session file is empty");
            }

            let mut header: Value =
                serde_json::from_str(&lines[0]).context("parsing session header")?;
            if header.get("type").and_then(|t| t.as_str()) != Some("session") {
                anyhow::bail!("Invalid session file: missing session header");
            }
            if header.get("deleted").and_then(|v| v.as_bool()) == Some(true) {
                return Ok(false);
            }
            header["deleted"] = Value::Bool(true);
            header["deleted_at"] = Value::String(Utc::now().to_rfc3339());
            lines[0] = serde_json::to_string(&header)?;
            let updated_text = lines.join("\n") + "\n";
            client
                .write_file(&session_path, updated_text.as_bytes(), false)
                .await
                .context("writing session file via runner")?;
            return Ok(true);
        }

        let session_path = self.find_session_file(&sessions_dir, session_id)?;
        let file = std::fs::File::open(&session_path).context("opening session file")?;
        let reader = BufReader::new(file);
        let mut lines: Vec<String> = reader.lines().collect::<std::io::Result<_>>()?;
        if lines.is_empty() {
            anyhow::bail!("Session file is empty");
        }

        let mut header: Value =
            serde_json::from_str(&lines[0]).context("parsing session header")?;
        if header.get("type").and_then(|t| t.as_str()) != Some("session") {
            anyhow::bail!("Invalid session file: missing session header");
        }
        if header.get("deleted").and_then(|v| v.as_bool()) == Some(true) {
            return Ok(false);
        }
        header["deleted"] = Value::Bool(true);
        header["deleted_at"] = Value::String(Utc::now().to_rfc3339());
        lines[0] = serde_json::to_string(&header)?;
        let updated_text = lines.join("\n") + "\n";
        std::fs::write(&session_path, updated_text).context("writing session file")?;
        Ok(true)
    }

    fn parse_session_reader<R: std::io::BufRead>(
        &self,
        mut reader: R,
        modified_ms: i64,
        source_path: Option<String>,
    ) -> Option<WorkspacePiSessionSummary> {
        let mut first_line = String::new();
        reader.read_line(&mut first_line).ok()?;
        if first_line.trim().is_empty() {
            return None;
        }

        let header: Value = serde_json::from_str(&first_line).ok()?;
        if header.get("type").and_then(|t| t.as_str()) != Some("session") {
            return None;
        }
        if header.get("deleted").and_then(|v| v.as_bool()) == Some(true) {
            return None;
        }

        let header_id = header.get("id").and_then(|v| v.as_str())?.to_string();
        let path_id = source_path
            .as_deref()
            .and_then(Self::session_id_from_path)
            .filter(|id| !id.is_empty());
        let id = match path_id {
            Some(path_id) if path_id != header_id => path_id,
            _ => header_id,
        };
        let timestamp = header
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let created_at = DateTime::parse_from_rfc3339(timestamp)
            .map(|dt| dt.timestamp_millis())
            .unwrap_or(modified_ms);
        let workspace_path = header
            .get("cwd")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .unwrap_or("global")
            .to_string();

        let mut title = header
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let parent_id = header
            .get("parentSession")
            .and_then(|v| v.as_str())
            .and_then(Self::read_parent_session_id);
        let version = match header.get("version") {
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Number(n)) => Some(n.to_string()),
            _ => None,
        };
        let header_readable_id = header
            .get("readable_id")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string());
        let mut session_info_name: Option<String> = None;

        for line in reader.lines().map_while(Result::ok) {
            if line.is_empty() {
                continue;
            }

            let entry: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if entry.get("type").and_then(|t| t.as_str()) == Some("session_info") {
                if let Some(name) = entry.get("name").and_then(|v| v.as_str()) {
                    let trimmed = name.trim();
                    if !trimmed.is_empty() {
                        session_info_name = Some(trimmed.to_string());
                    }
                }
                continue;
            }

            if title.is_some() {
                continue;
            }

            if entry.get("type").and_then(|t| t.as_str()) != Some("message") {
                continue;
            }

            if let Some(msg) = entry.get("message")
                && msg.get("role").and_then(|r| r.as_str()) == Some("user")
                && let Some(content) = msg.get("content")
            {
                title = Self::extract_title_from_content(content);
                break;
            }
        }

        if let Some(info_name) = session_info_name {
            title = Some(info_name);
        }

        let mut readable_id = header_readable_id;
        if let Some(current_title) = title.as_deref() {
            let parsed = crate::pi::session_parser::ParsedTitle::parse(current_title);
            if let Some(parsed_readable) = parsed.readable_id.clone() {
                readable_id = Some(parsed_readable);
            }
            let cleaned = parsed.display_title().trim();
            if !cleaned.is_empty() {
                title = Some(cleaned.to_string());
            }
        }

        Some(WorkspacePiSessionSummary {
            id,
            title,
            readable_id,
            parent_id,
            workspace_path,
            created_at,
            updated_at: modified_ms,
            version,
            source_path,
        })
    }

    fn parse_session_file(&self, path: &Path) -> Option<WorkspacePiSessionSummary> {
        use std::io::BufReader;

        let file = std::fs::File::open(path).ok()?;
        let metadata = file.metadata().ok()?;
        let modified = metadata.modified().ok()?;
        let modified_ms = modified
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis() as i64;

        let reader = BufReader::new(file);
        self.parse_session_reader(reader, modified_ms, Some(path.to_string_lossy().to_string()))
    }

    fn session_id_from_path(path: &str) -> Option<String> {
        let stem = std::path::Path::new(path)
            .file_stem()?
            .to_string_lossy();
        let mut parts = stem.rsplitn(2, '_');
        let id = parts.next()?.trim();
        if id.is_empty() {
            None
        } else {
            Some(id.to_string())
        }
    }

    fn read_parent_session_id(path: &str) -> Option<String> {
        use std::io::{BufRead, BufReader};

        let file = std::fs::File::open(path).ok()?;
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        if line.trim().is_empty() {
            return None;
        }
        let header: Value = serde_json::from_str(&line).ok()?;
        if header.get("type").and_then(|t| t.as_str()) != Some("session") {
            return None;
        }
        header
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    fn extract_title_from_content(content: &Value) -> Option<String> {
        if let Some(text) = content.as_str() {
            return Some(Self::truncate_title(text));
        }

        if let Some(arr) = content.as_array() {
            for block in arr {
                if block.get("type").and_then(|t| t.as_str()) == Some("text")
                    && let Some(text) = block.get("text").and_then(|t| t.as_str())
                {
                    return Some(Self::truncate_title(text));
                }
            }
        }

        None
    }

    fn truncate_title(text: &str) -> String {
        let text = text.trim();
        if text.len() <= 50 {
            text.to_string()
        } else {
            format!("{}...", &text[..47])
        }
    }

    async fn create_session(
        &self,
        user_id: &str,
        work_dir: &Path,
        session_file: Option<PathBuf>,
    ) -> Result<UserPiSession> {
        if !work_dir.exists() {
            anyhow::bail!("Workspace directory does not exist: {:?}", work_dir);
        }

        let mut append_system_prompt = Vec::new();
        let onboard_file = work_dir.join("ONBOARD.md");
        if onboard_file.exists() {
            append_system_prompt.push(onboard_file);
        }
        let personality_file = work_dir.join("PERSONALITY.md");
        if personality_file.exists() {
            append_system_prompt.push(personality_file);
        }
        let user_file = work_dir.join("USER.md");
        if user_file.exists() {
            append_system_prompt.push(user_file);
        }

        let spawn_config = PiSpawnConfig {
            work_dir: work_dir.to_path_buf(),
            pi_executable: self.config.pi_executable.clone(),
            continue_session: false,
            session_file,
            provider: self.config.default_provider.clone(),
            model: self.config.default_model.clone(),
            extensions: self.config.extensions.clone(),
            append_system_prompt,
            env: HashMap::new(),
            sandboxed: self.config.sandboxed,
        };

        let runtime = self.create_runtime_for_user(user_id);
        let process = runtime.spawn(spawn_config).await.with_context(|| {
            format!(
                "Failed to spawn Pi process for user {} in {:?}",
                user_id, work_dir
            )
        })?;

        Ok(UserPiSession::from_process(process))
    }

    /// Start a new Pi session for a workspace and return its session id.
    pub async fn start_new_session(
        &self,
        user_id: &str,
        work_dir: &Path,
    ) -> Result<(String, Arc<UserPiSession>)> {
        let session_id = Uuid::new_v4().to_string();
        let session_file = self
            .ensure_session_file(user_id, work_dir, &session_id)
            .await?;
        let session = self
            .create_session(user_id, work_dir, Some(session_file))
            .await?;
        let session = Arc::new(session);

        if let Ok(state) = session.get_state().await {
            if let Some(actual_id) = state.session_id
                && actual_id != session_id
            {
                warn!(
                    "Pi session_id mismatch (requested {}, got {}). Using requested id for tracking.",
                    session_id, actual_id
                );
            }
        }

        let key = (
            user_id.to_string(),
            work_dir.to_string_lossy().to_string(),
            session_id.clone(),
        );
        let mut sessions = self.sessions.write().await;
        sessions.insert(key, Arc::clone(&session));

        if let Err(err) = session.set_auto_retry(true).await {
            warn!(
                "Failed to enable auto-retry for workspace session {}: {}",
                session_id, err
            );
        }

        Ok((session_id, session))
    }

    /// Resume a Pi session by ID for a workspace.
    ///
    /// Uses a creation lock to prevent duplicate process spawns from concurrent requests.
    pub async fn resume_session(
        &self,
        user_id: &str,
        work_dir: &Path,
        session_id: &str,
    ) -> Result<Arc<UserPiSession>> {
        let key = (
            user_id.to_string(),
            work_dir.to_string_lossy().to_string(),
            session_id.to_string(),
        );

        // Fast path: check if session already exists
        {
            let sessions = self.sessions.read().await;
            if let Some(existing) = sessions.get(&key) {
                return Ok(Arc::clone(existing));
            }
        }

        // Acquire creation lock to prevent duplicate spawns from concurrent requests.
        // If another request is already creating this session, wait and then return the result.
        {
            let mut creating = self.creating.lock().await;
            if creating.contains(&key) {
                // Another request is creating this session - drop lock and wait
                drop(creating);
                // Poll until the session appears in the cache
                for _ in 0..50 {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    let sessions = self.sessions.read().await;
                    if let Some(existing) = sessions.get(&key) {
                        return Ok(Arc::clone(existing));
                    }
                }
                anyhow::bail!("Timed out waiting for concurrent session creation");
            }
            // Mark this key as being created
            creating.insert(key.clone());
        }

        // Create the session (we hold the creation slot)
        let result = async {
            let sessions_dir = self.get_pi_sessions_dir(user_id, work_dir);
            let session_file = match self.find_session_file(&sessions_dir, session_id) {
                Ok(path) => path,
                Err(err) if err.to_string().contains("Session not found") => {
                    if let Ok(Some(path)) = self
                        .rehydrate_session_from_hstry(user_id, work_dir, session_id)
                        .await
                    {
                        path
                    } else {
                        return Err(err);
                    }
                }
                Err(err) => return Err(err),
            };
            let session = self
                .create_session(user_id, work_dir, Some(session_file))
                .await?;
            let session = Arc::new(session);

            if let Err(err) = session.set_auto_retry(true).await {
                warn!(
                    "Failed to enable auto-retry for workspace session {}: {}",
                    session_id, err
                );
            }

            let mut sessions = self.sessions.write().await;
            sessions.insert(key.clone(), Arc::clone(&session));

            Ok(session)
        }
        .await;

        // Always remove from creating set, even on error
        {
            let mut creating = self.creating.lock().await;
            creating.remove(&key);
        }

        result
    }

    /// Get a running session if it exists.
    pub async fn get_session(
        &self,
        user_id: &str,
        work_dir: &Path,
        session_id: &str,
    ) -> Option<Arc<UserPiSession>> {
        let key = (
            user_id.to_string(),
            work_dir.to_string_lossy().to_string(),
            session_id.to_string(),
        );
        let sessions = self.sessions.read().await;
        sessions.get(&key).cloned()
    }

    /// Get messages from a specific Pi session file.
    pub fn get_session_messages(
        &self,
        user_id: &str,
        work_dir: &Path,
        session_id: &str,
    ) -> Result<Vec<PiSessionMessage>> {
        use std::io::{BufRead, BufReader};

        let sessions_dir = self.get_pi_sessions_dir(user_id, work_dir);
        let session_file = self.find_session_file(&sessions_dir, session_id)?;

        let file = std::fs::File::open(&session_file).context("opening session file")?;
        let reader = BufReader::new(file);

        let mut messages = Vec::new();
        for line in reader.lines().map_while(Result::ok) {
            if line.is_empty() {
                continue;
            }

            if let Ok(entry) = serde_json::from_str::<Value>(&line)
                && entry.get("type").and_then(|t| t.as_str()) == Some("message")
                && let Some(msg) = entry.get("message")
            {
                let id = entry
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let role = msg
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("user")
                    .to_string();
                let content = msg.get("content").cloned().unwrap_or(Value::Null);
                let tool_call_id = msg
                    .get("toolCallId")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string());
                let tool_name = msg
                    .get("toolName")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string());
                let is_error = msg.get("isError").and_then(|v| v.as_bool());
                let timestamp = msg.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
                let usage = msg.get("usage").cloned();

                messages.push(PiSessionMessage {
                    id,
                    role,
                    content,
                    tool_call_id,
                    tool_name,
                    is_error,
                    timestamp,
                    usage,
                });
            }
        }

        Ok(messages)
    }

    /// List all workspace Pi sessions from disk for the current user.
    pub fn list_sessions_for_user(&self, _user_id: &str) -> Result<Vec<WorkspacePiSessionSummary>> {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let sessions_root = home.join(".pi").join("agent").join("sessions");
        if !sessions_root.exists() {
            return Ok(Vec::new());
        }

        let mut sessions_by_id: std::collections::HashMap<String, WorkspacePiSessionSummary> =
            std::collections::HashMap::new();
        let roots = std::fs::read_dir(&sessions_root)
            .with_context(|| format!("reading Pi sessions root: {:?}", sessions_root))?;

        for root in roots.filter_map(|e| e.ok()) {
            let root_path = root.path();
            if !root_path.is_dir() {
                continue;
            }

            let entries = match std::fs::read_dir(&root_path) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map(|e| e == "jsonl").unwrap_or(false)
                    && let Some(session) = self.parse_session_file(&path)
                {
                    sessions_by_id
                        .entry(session.id.clone())
                        .and_modify(|existing| {
                            if session.updated_at > existing.updated_at {
                                *existing = session.clone();
                            }
                        })
                        .or_insert(session);
                }
            }
        }

        let mut sessions: Vec<WorkspacePiSessionSummary> =
            sessions_by_id.into_values().collect();
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    /// Update a workspace Pi session title by editing the JSONL header.
    pub async fn update_session_title(
        &self,
        user_id: &str,
        work_dir: &Path,
        session_id: &str,
        title: &str,
    ) -> Result<WorkspacePiSessionSummary> {
        use std::io::{BufRead, BufReader};

        let sessions_dir = self.get_pi_sessions_dir(user_id, work_dir);
        let title = title.trim();
        if title.is_empty() {
            anyhow::bail!("Title cannot be empty");
        }

        if let Some(client) = self.runner_client_for_user(user_id) {
            let listing = client
                .list_directory(&sessions_dir, false)
                .await
                .context("listing session directory via runner")?;
            let entry = listing
                .entries
                .iter()
                .find(|entry| {
                    !entry.is_dir
                        && entry.name.ends_with(".jsonl")
                        && entry.name.contains(session_id)
                })
                .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;
            let session_path = sessions_dir.join(&entry.name);

            let content = client
                .read_file(&session_path, None, None)
                .await
                .context("reading session file via runner")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(content.content_base64)
                .context("decoding session file base64")?;
            let reader = BufReader::new(std::io::Cursor::new(bytes));
            let mut lines: Vec<String> = reader.lines().collect::<std::io::Result<_>>()?;
            if lines.is_empty() {
                anyhow::bail!("Session file is empty");
            }

            let mut header: Value =
                serde_json::from_str(&lines[0]).context("parsing session header")?;
            if header.get("type").and_then(|t| t.as_str()) != Some("session") {
                anyhow::bail!("Invalid session file: missing session header");
            }
            header["title"] = Value::String(title.to_string());
            lines[0] = serde_json::to_string(&header)?;
            let updated_text = lines.join("\n") + "\n";
            client
                .write_file(&session_path, updated_text.as_bytes(), false)
                .await
                .context("writing session file via runner")?;

            let modified_ms = Utc::now().timestamp_millis();
            let reader = BufReader::new(std::io::Cursor::new(updated_text.into_bytes()));
            return self
                .parse_session_reader(reader, modified_ms, None)
                .ok_or_else(|| anyhow::anyhow!("Failed to parse updated session"));
        }

        let session_path = self.find_session_file(&sessions_dir, session_id)?;
        let file = std::fs::File::open(&session_path).context("opening session file")?;
        let reader = BufReader::new(file);
        let mut lines: Vec<String> = reader.lines().collect::<std::io::Result<_>>()?;
        if lines.is_empty() {
            anyhow::bail!("Session file is empty");
        }

        let mut header: Value =
            serde_json::from_str(&lines[0]).context("parsing session header")?;
        if header.get("type").and_then(|t| t.as_str()) != Some("session") {
            anyhow::bail!("Invalid session file: missing session header");
        }
        header["title"] = Value::String(title.to_string());
        lines[0] = serde_json::to_string(&header)?;
        let updated_text = lines.join("\n") + "\n";
        std::fs::write(&session_path, updated_text).context("writing session file")?;

        self.parse_session_file(&session_path)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse updated session"))
    }
}
