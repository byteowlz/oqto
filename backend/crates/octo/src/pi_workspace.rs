//! Pi workspace sessions service.
//!
//! Manages one Pi process per workspace session (per user), with idle cleanup.

use anyhow::{Context, Result};
use chrono::DateTime;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};

use crate::main_chat::{MainChatPiServiceConfig, PiRuntimeMode, UserPiSession};
use crate::pi::{ContainerPiRuntime, LocalPiRuntime, PiRuntime, PiSpawnConfig, RunnerPiRuntime};
use crate::runner::client::RunnerClient;

/// Default idle timeout for sessions (5 minutes).
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 300;

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
}

impl WorkspacePiService {
    pub fn new(config: MainChatPiServiceConfig) -> Self {
        info!(
            "WorkspacePiService initialized with runtime mode: {}",
            config.runtime_mode
        );
        Self {
            config,
            sessions: RwLock::new(HashMap::new()),
            creating: Mutex::new(HashSet::new()),
            idle_timeout_secs: DEFAULT_IDLE_TIMEOUT_SECS,
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
                    let socket_path = pattern.replace("{user}", user_id);
                    RunnerClient::new(socket_path)
                } else {
                    RunnerClient::default()
                };
                Arc::new(RunnerPiRuntime::new(client))
            }
            PiRuntimeMode::Container => Arc::new(ContainerPiRuntime::new()),
        }
    }

    /// Get the Pi agent directory for a working directory.
    fn get_pi_agent_dir(&self, _work_dir: &Path) -> PathBuf {
        dirs::home_dir()
            .map(|home| home.join(".pi").join("agent"))
            .unwrap_or_else(|| PathBuf::from(".pi/agent"))
    }

    /// Get the Pi sessions directory for a working directory.
    /// Pi stores sessions in ~/.pi/agent/sessions/{escaped-path}/
    fn get_pi_sessions_dir(&self, work_dir: &Path) -> PathBuf {
        let escaped_path = work_dir
            .to_string_lossy()
            .replace('/', "-")
            .trim_start_matches('-')
            .to_string();
        self.get_pi_agent_dir(work_dir)
            .join("sessions")
            .join(format!("--{}--", escaped_path))
    }

    /// Find a Pi session file by ID (.jsonl format).
    fn find_session_file(&self, sessions_dir: &Path, session_id: &str) -> Result<PathBuf> {
        if !sessions_dir.exists() {
            anyhow::bail!("Sessions directory not found");
        }

        let entries = std::fs::read_dir(sessions_dir).context("reading sessions directory")?;
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if filename.contains(session_id) {
                    return Ok(path);
                }
            }
        }

        anyhow::bail!("Session not found: {}", session_id)
    }

    fn parse_session_file(&self, path: &Path) -> Option<WorkspacePiSessionSummary> {
        use std::io::{BufRead, BufReader};

        let file = std::fs::File::open(path).ok()?;
        let metadata = file.metadata().ok()?;
        let modified = metadata.modified().ok()?;
        let modified_ms = modified
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis() as i64;

        let mut reader = BufReader::new(file);
        let mut first_line = String::new();
        reader.read_line(&mut first_line).ok()?;
        if first_line.trim().is_empty() {
            return None;
        }

        let header: Value = serde_json::from_str(&first_line).ok()?;
        if header.get("type").and_then(|t| t.as_str()) != Some("session") {
            return None;
        }

        let id = header.get("id").and_then(|v| v.as_str())?.to_string();
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

        if title.is_none() {
            for line in reader.lines().filter_map(|l| l.ok()) {
                if line.is_empty() {
                    continue;
                }

                let entry: Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                if entry.get("type").and_then(|t| t.as_str()) != Some("message") {
                    continue;
                }

                if let Some(msg) = entry.get("message") {
                    if msg.get("role").and_then(|r| r.as_str()) == Some("user") {
                        if let Some(content) = msg.get("content") {
                            title = Self::extract_title_from_content(content);
                            break;
                        }
                    }
                }
            }
        }

        Some(WorkspacePiSessionSummary {
            id,
            title,
            parent_id,
            workspace_path,
            created_at,
            updated_at: modified_ms,
            version,
            source_path: Some(path.to_string_lossy().to_string()),
        })
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
                if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        return Some(Self::truncate_title(text));
                    }
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
        let session = self.create_session(user_id, work_dir, None).await?;
        let session = Arc::new(session);

        let state = session.get_state().await?;
        let session_id = state
            .session_id
            .ok_or_else(|| anyhow::anyhow!("Pi session_id missing from state"))?;

        let key = (
            user_id.to_string(),
            work_dir.to_string_lossy().to_string(),
            session_id.clone(),
        );
        let mut sessions = self.sessions.write().await;
        sessions.insert(key, Arc::clone(&session));

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
            let sessions_dir = self.get_pi_sessions_dir(&work_dir.to_path_buf());
            let session_file = self.find_session_file(&sessions_dir, session_id)?;
            let session = self
                .create_session(user_id, work_dir, Some(session_file))
                .await?;
            let session = Arc::new(session);

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
        work_dir: &Path,
        session_id: &str,
    ) -> Result<Vec<PiSessionMessage>> {
        use std::io::{BufRead, BufReader};

        let sessions_dir = self.get_pi_sessions_dir(&work_dir.to_path_buf());
        let session_file = self.find_session_file(&sessions_dir, session_id)?;

        let file = std::fs::File::open(&session_file).context("opening session file")?;
        let reader = BufReader::new(file);

        let mut messages = Vec::new();
        for line in reader.lines().filter_map(|l| l.ok()) {
            if line.is_empty() {
                continue;
            }

            if let Ok(entry) = serde_json::from_str::<Value>(&line) {
                if entry.get("type").and_then(|t| t.as_str()) == Some("message") {
                    if let Some(msg) = entry.get("message") {
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

        let mut sessions = Vec::new();
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
                    if let Some(session) = self.parse_session_file(&path) {
                        sessions.push(session);
                    }
                }
            }
        }

        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }
}
