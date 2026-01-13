//! Pi agent service for Main Chat.
//!
//! Manages Pi subprocesses for each user's Main Chat. Each user gets one Pi
//! process that persists across requests, enabling streaming and maintaining
//! session state.
//!
//! ## Session Lifecycle
//!
//! Sessions are managed with smart continuation vs fresh start logic:
//! - **Continue** if: last activity < 4 hours AND session file < 500KB
//! - **Fresh start** if: session is stale, too large, or user requests new session
//!
//! On fresh start, context is injected from:
//! 1. Last session's compaction summary (from main_chat.db)
//! 2. Recent mmry entries (decisions, handoffs, insights)
//!
//! ## Runtime Modes
//!
//! Pi can run in different isolation modes:
//! - **Local**: Direct subprocess on host (single-user mode)
//! - **Runner**: Via octo-runner daemon (multi-user isolation)
//! - **Container**: HTTP client to pi-bridge in container

use anyhow::{Context, Result};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{Mutex, RwLock, broadcast};

use crate::pi::{
    AgentMessage, AssistantMessageEvent, CompactionResult, ContainerPiRuntime, LocalPiRuntime,
    PiCommand, PiEvent, PiProcess, PiRuntime, PiSpawnConfig, PiState, RunnerPiRuntime, SessionStats,
};
use crate::runner::client::RunnerClient;

/// Session freshness thresholds
const SESSION_MAX_AGE_HOURS: u64 = 4;
const SESSION_MAX_SIZE_BYTES: u64 = 500 * 1024; // 500KB

/// Pi runtime mode determines how Pi processes are spawned and isolated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PiRuntimeMode {
    /// Direct subprocess on host (single-user mode, no isolation).
    #[default]
    Local,
    /// Via octo-runner daemon (multi-user isolation, processes run as separate Linux users).
    Runner,
    /// HTTP client to pi-bridge in container (container mode).
    Container,
}

impl std::fmt::Display for PiRuntimeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PiRuntimeMode::Local => write!(f, "local"),
            PiRuntimeMode::Runner => write!(f, "runner"),
            PiRuntimeMode::Container => write!(f, "container"),
        }
    }
}

/// Configuration for the Pi service.
#[derive(Debug, Clone)]
pub struct MainChatPiServiceConfig {
    /// Path to the Pi CLI executable (e.g., "pi" or "/usr/local/bin/pi")
    pub pi_executable: String,
    /// Default provider for new sessions
    pub default_provider: Option<String>,
    /// Default model for new sessions
    pub default_model: Option<String>,
    /// Extension files to load (passed via --extension)
    pub extensions: Vec<String>,
    /// Maximum session age before forcing fresh start (hours)
    pub max_session_age_hours: u64,
    /// Maximum session file size before forcing fresh start (bytes)
    pub max_session_size_bytes: u64,
    /// Runtime mode for Pi process isolation.
    pub runtime_mode: PiRuntimeMode,
    /// Runner socket path pattern (for Runner mode).
    /// Use {user} placeholder for username, e.g., "/run/octo/runner-{user}.sock"
    pub runner_socket_pattern: Option<String>,
    /// Pi bridge URL (for Container mode).
    /// e.g., "http://localhost:41824"
    pub bridge_url: Option<String>,
}

impl Default for MainChatPiServiceConfig {
    fn default() -> Self {
        Self {
            pi_executable: "pi".to_string(),
            default_provider: None,
            default_model: None,
            extensions: Vec::new(),
            max_session_age_hours: SESSION_MAX_AGE_HOURS,
            max_session_size_bytes: SESSION_MAX_SIZE_BYTES,
            runtime_mode: PiRuntimeMode::Local,
            runner_socket_pattern: None,
            bridge_url: None,
        }
    }
}

/// Information about the last Pi session for a directory.
#[derive(Debug, Clone)]
pub struct LastSessionInfo {
    /// File size in bytes
    pub size: u64,
    /// Last modification time
    pub modified: SystemTime,
}

#[derive(Debug, Clone)]
enum StreamPart {
    Text(String),
    Thinking(String),
    ToolUse { id: String, name: String, input: Value },
    ToolResult {
        id: String,
        name: Option<String>,
        content: Value,
        is_error: bool,
    },
}

#[derive(Debug, Default, Clone)]
struct StreamSnapshot {
    is_streaming: bool,
    has_message: bool,
    parts: Vec<StreamPart>,
}

impl StreamSnapshot {
    fn reset(&mut self) {
        self.is_streaming = false;
        self.has_message = false;
        self.parts.clear();
    }

    fn push_text(&mut self, delta: &str) {
        match self.parts.last_mut() {
            Some(StreamPart::Text(existing)) => existing.push_str(delta),
            _ => self.parts.push(StreamPart::Text(delta.to_string())),
        }
    }

    fn push_thinking(&mut self, delta: &str) {
        match self.parts.last_mut() {
            Some(StreamPart::Thinking(existing)) => existing.push_str(delta),
            _ => self
                .parts
                .push(StreamPart::Thinking(delta.to_string())),
        }
    }

    fn apply_event(&mut self, event: &PiEvent) {
        match event {
            PiEvent::AgentStart => {
                self.is_streaming = true;
            }
            PiEvent::AgentEnd { .. } => {
                self.reset();
            }
            PiEvent::MessageStart { message } => {
                if message.role == "assistant" {
                    self.is_streaming = true;
                    self.has_message = true;
                    self.parts.clear();
                }
            }
            PiEvent::MessageUpdate {
                assistant_message_event,
                message,
            } => match assistant_message_event {
                AssistantMessageEvent::TextDelta { delta, .. } => {
                    if message.role == "assistant" {
                        self.is_streaming = true;
                        self.has_message = true;
                    }
                    self.push_text(delta);
                }
                AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                    if message.role == "assistant" {
                        self.is_streaming = true;
                        self.has_message = true;
                    }
                    self.push_thinking(delta);
                }
                AssistantMessageEvent::ToolcallEnd { tool_call, .. } => {
                    if message.role == "assistant" {
                        self.is_streaming = true;
                        self.has_message = true;
                    }
                    self.parts.push(StreamPart::ToolUse {
                        id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        input: tool_call.arguments.clone(),
                    });
                }
                _ => {}
            },
            PiEvent::ToolExecutionEnd {
                tool_call_id,
                tool_name,
                result,
                is_error,
            } => {
                let content = serde_json::to_value(result).unwrap_or(Value::Null);
                self.parts.push(StreamPart::ToolResult {
                    id: tool_call_id.clone(),
                    name: Some(tool_name.clone()),
                    content,
                    is_error: *is_error,
                });
            }
            _ => {}
        }
    }

    fn to_ws_events(&self) -> Vec<Value> {
        if !self.is_streaming || !self.has_message {
            return Vec::new();
        }

        let mut events = Vec::with_capacity(self.parts.len() + 1);
        events.push(json!({"type": "message_start", "role": "assistant"}));

        for part in &self.parts {
            match part {
                StreamPart::Text(text) => {
                    events.push(json!({"type": "text", "data": text}));
                }
                StreamPart::Thinking(text) => {
                    events.push(json!({"type": "thinking", "data": text}));
                }
                StreamPart::ToolUse { id, name, input } => {
                    events.push(json!({
                        "type": "tool_use",
                        "data": {
                            "id": id,
                            "name": name,
                            "input": input
                        }
                    }));
                }
                StreamPart::ToolResult {
                    id,
                    name,
                    content,
                    is_error,
                } => {
                    events.push(json!({
                        "type": "tool_result",
                        "data": {
                            "id": id,
                            "name": name,
                            "content": content,
                            "isError": is_error
                        }
                    }));
                }
            }
        }

        events
    }
}

/// Handle to a user's Pi session.
pub struct UserPiSession {
    /// The Pi process for this user (trait object for runtime polymorphism).
    process: Arc<tokio::sync::RwLock<Box<dyn PiProcess>>>,
    /// Snapshot of the currently streaming assistant message for WS replay.
    stream_snapshot: Arc<Mutex<StreamSnapshot>>,
}

/// Service for managing Pi sessions for Main Chat users.
pub struct MainChatPiService {
    /// Configuration.
    config: MainChatPiServiceConfig,
    /// Active sessions (keyed by user_id).
    sessions: RwLock<HashMap<String, Arc<UserPiSession>>>,
    /// Base workspace directory.
    workspace_dir: PathBuf,
    /// Single-user mode.
    single_user: bool,
}

impl MainChatPiService {
    /// Create a new Pi service.
    pub fn new(workspace_dir: PathBuf, single_user: bool, config: MainChatPiServiceConfig) -> Self {
        info!(
            "MainChatPiService initialized with runtime mode: {}",
            config.runtime_mode
        );

        Self {
            config,
            sessions: RwLock::new(HashMap::new()),
            workspace_dir,
            single_user,
        }
    }

    /// Create a runtime for a specific user (needed for Runner mode).
    fn create_runtime_for_user(&self, user_id: &str) -> Arc<dyn PiRuntime> {
        match self.config.runtime_mode {
            PiRuntimeMode::Local => Arc::new(LocalPiRuntime::new()),
            PiRuntimeMode::Runner => {
                // Create runner client - uses XDG_RUNTIME_DIR by default
                let client = if let Some(pattern) = self.config.runner_socket_pattern.as_deref() {
                    RunnerClient::new(pattern.replace("{user}", user_id))
                } else {
                    RunnerClient::default()
                };
                debug!(
                    "Runner socket for user {}: {:?}",
                    user_id,
                    client.socket_path()
                );
                Arc::new(RunnerPiRuntime::new(client))
            }
            PiRuntimeMode::Container => Arc::new(ContainerPiRuntime::new()),
        }
    }

    /// Get the Main Chat directory for a user.
    fn get_main_chat_dir(&self, user_id: &str) -> PathBuf {
        if self.single_user {
            self.workspace_dir.join("main")
        } else {
            self.workspace_dir.join(user_id).join("main")
        }
    }

    /// Get the Pi sessions directory for a working directory.
    /// Pi stores sessions in ~/.pi/agent/sessions/{escaped-path}/
    fn get_pi_sessions_dir(&self, work_dir: &PathBuf) -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let escaped_path = work_dir
            .to_string_lossy()
            .replace('/', "-")
            .trim_start_matches('-')
            .to_string();
        home.join(".pi")
            .join("agent")
            .join("sessions")
            .join(format!("-{}-", escaped_path))
    }

    /// Find the most recent Pi session file for a directory.
    fn find_last_session(&self, work_dir: &PathBuf) -> Option<LastSessionInfo> {
        let sessions_dir = self.get_pi_sessions_dir(work_dir);

        if !sessions_dir.exists() {
            debug!("Pi sessions directory does not exist: {:?}", sessions_dir);
            return None;
        }

        let mut latest: Option<LastSessionInfo> = None;

        if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            let info = LastSessionInfo {
                                size: metadata.len(),
                                modified,
                            };

                            if latest
                                .as_ref()
                                .map(|l| modified > l.modified)
                                .unwrap_or(true)
                            {
                                latest = Some(info);
                            }
                        }
                    }
                }
            }
        }

        latest
    }

    /// Check if a session should be continued or if we need a fresh start.
    fn should_continue_session(&self, last_session: &LastSessionInfo) -> bool {
        let now = SystemTime::now();

        // Check age
        let age = now
            .duration_since(last_session.modified)
            .unwrap_or(Duration::MAX);
        let max_age = Duration::from_secs(self.config.max_session_age_hours * 3600);

        if age > max_age {
            info!(
                "Session too old ({:?} > {:?}), starting fresh",
                age, max_age
            );
            return false;
        }

        // Check size
        if last_session.size > self.config.max_session_size_bytes {
            info!(
                "Session file too large ({} > {} bytes), starting fresh",
                last_session.size, self.config.max_session_size_bytes
            );
            return false;
        }

        info!(
            "Session is fresh (age={:?}, size={}), continuing",
            age, last_session.size
        );
        true
    }

    /// Get or create a Pi session for a user.
    pub async fn get_or_create_session(&self, user_id: &str) -> Result<Arc<UserPiSession>> {
        // Check if session exists
        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(user_id) {
                return Ok(Arc::clone(session));
            }
        }

        // Create new session
        let session = self.create_session(user_id, false).await?;
        let session = Arc::new(session);

        // Store in cache
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(user_id.to_string(), Arc::clone(&session));
        }

        Ok(session)
    }

    /// Create a new Pi session for a user.
    ///
    /// # Arguments
    /// * `user_id` - The user ID
    /// * `force_fresh` - If true, always start a fresh session regardless of staleness
    async fn create_session(&self, user_id: &str, force_fresh: bool) -> Result<UserPiSession> {
        let work_dir = self.get_main_chat_dir(user_id);

        // Ensure the directory exists
        if !work_dir.exists() {
            anyhow::bail!("Main Chat directory does not exist for user: {}", user_id);
        }

        // Determine if we should continue or start fresh
        let last_session = self.find_last_session(&work_dir);
        let should_continue = !force_fresh
            && last_session
                .as_ref()
                .map(|s| self.should_continue_session(s))
                .unwrap_or(false);

        info!(
            "Starting Pi session for user {} in {:?}, continue={}, provider={:?}, model={:?}, mode={}",
            user_id,
            work_dir,
            should_continue,
            self.config.default_provider,
            self.config.default_model,
            self.config.runtime_mode
        );

        // Build system prompt files
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

        // Build environment for container mode
        let mut env = HashMap::new();
        if let Some(ref bridge_url) = self.config.bridge_url {
            env.insert("PI_BRIDGE_URL".to_string(), bridge_url.clone());
        }

        // Build spawn config
        let spawn_config = PiSpawnConfig {
            work_dir: work_dir.clone(),
            pi_executable: self.config.pi_executable.clone(),
            continue_session: should_continue,
            provider: self.config.default_provider.clone(),
            model: self.config.default_model.clone(),
            extensions: self.config.extensions.clone(),
            append_system_prompt,
            env,
        };

        // Get the appropriate runtime for this user
        let runtime = self.create_runtime_for_user(user_id);

        // Spawn the process
        let process = runtime.spawn(spawn_config).await.with_context(|| {
            format!(
                "Failed to spawn Pi process for user {} in {:?}",
                user_id, work_dir
            )
        })?;

        let stream_snapshot = Arc::new(Mutex::new(StreamSnapshot::default()));
        let stream_snapshot_task = Arc::clone(&stream_snapshot);
        let mut event_rx = process.subscribe();
        tokio::spawn(async move {
            loop {
                match event_rx.recv().await {
                    Ok(event) => {
                        let mut snapshot = stream_snapshot_task.lock().await;
                        snapshot.apply_event(&event);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let mut snapshot = stream_snapshot_task.lock().await;
                        snapshot.reset();
                    }
                }
            }
        });

        Ok(UserPiSession {
            process: Arc::new(tokio::sync::RwLock::new(process)),
            stream_snapshot,
        })
    }

    /// Close a user's Pi session.
    pub async fn close_session(&self, user_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(_session) = sessions.remove(user_id) {
            info!("Closed Pi session for user {}", user_id);
            // The session will be dropped, which should clean up the process
        }
        Ok(())
    }

    /// Get session if it exists (without creating).
    pub async fn get_session(&self, user_id: &str) -> Option<Arc<UserPiSession>> {
        let sessions = self.sessions.read().await;
        sessions.get(user_id).cloned()
    }

    /// Check if a session exists for a user.
    pub async fn has_session(&self, user_id: &str) -> bool {
        let sessions = self.sessions.read().await;
        sessions.contains_key(user_id)
    }

    /// Reset a user's Pi session - closes the current session and creates a fresh one.
    /// This re-reads PERSONALITY.md and USER.md files.
    pub async fn reset_session(&self, user_id: &str) -> Result<Arc<UserPiSession>> {
        // Close existing session if any
        self.close_session(user_id).await?;

        // Create a fresh session (force_fresh=true ensures no --continue flag)
        let session = self.create_session(user_id, true).await?;
        let session = Arc::new(session);

        // Store in cache
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(user_id.to_string(), Arc::clone(&session));
        }

        info!("Reset Pi session for user {}", user_id);
        Ok(session)
    }
}

impl UserPiSession {
    /// Send a prompt to the agent.
    pub async fn prompt(&self, message: &str) -> Result<()> {
        let process = self.process.read().await;
        process
            .send_command(PiCommand::Prompt {
                id: None,
                message: message.to_string(),
            })
            .await?;
        Ok(())
    }

    /// Abort the current operation.
    pub async fn abort(&self) -> Result<()> {
        let process = self.process.read().await;
        process.send_command(PiCommand::Abort { id: None }).await?;
        Ok(())
    }

    /// Queue a steering message to interrupt the agent mid-run.
    pub async fn steer(&self, message: &str) -> Result<()> {
        let process = self.process.read().await;
        process
            .send_command(PiCommand::Steer {
                id: None,
                message: message.to_string(),
            })
            .await?;
        Ok(())
    }

    /// Queue a follow-up message for after the agent finishes.
    pub async fn follow_up(&self, message: &str) -> Result<()> {
        let process = self.process.read().await;
        process
            .send_command(PiCommand::FollowUp {
                id: None,
                message: message.to_string(),
            })
            .await?;
        Ok(())
    }

    /// Get current state.
    pub async fn get_state(&self) -> Result<PiState> {
        let process = self.process.read().await;
        let response = process
            .send_command(PiCommand::GetState { id: None })
            .await?;
        if !response.success {
            anyhow::bail!("get_state failed: {:?}", response.error);
        }
        let data = response.data.context("get_state returned no data")?;
        serde_json::from_value(data).context("failed to parse state")
    }

    /// Get all messages.
    pub async fn get_messages(&self) -> Result<Vec<AgentMessage>> {
        let process = self.process.read().await;
        let response = process
            .send_command(PiCommand::GetMessages { id: None })
            .await?;
        if !response.success {
            anyhow::bail!("get_messages failed: {:?}", response.error);
        }
        let data = response.data.context("get_messages returned no data")?;
        let messages_data = data
            .get("messages")
            .context("no messages field in response")?;
        serde_json::from_value(messages_data.clone()).context("failed to parse messages")
    }

    /// Subscribe to events.
    ///
    /// Note: This requires awaiting to get the process lock, then returns
    /// the receiver synchronously.
    pub async fn subscribe(&self) -> broadcast::Receiver<PiEvent> {
        let process = self.process.read().await;
        process.subscribe()
    }

    /// Get the current streaming snapshot as WS events (for replay on reconnect).
    pub async fn stream_snapshot_events(&self) -> Vec<Value> {
        let snapshot = self.stream_snapshot.lock().await;
        snapshot.to_ws_events()
    }

    /// Compact the session context.
    pub async fn compact(&self, custom_instructions: Option<&str>) -> Result<CompactionResult> {
        let process = self.process.read().await;
        let response = process
            .send_command(PiCommand::Compact {
                id: None,
                custom_instructions: custom_instructions.map(|s| s.to_string()),
            })
            .await?;
        if !response.success {
            anyhow::bail!("compact failed: {:?}", response.error);
        }
        let data = response.data.context("compact returned no data")?;
        serde_json::from_value(data).context("failed to parse compaction result")
    }

    /// Start a new session (clear history).
    pub async fn new_session(&self) -> Result<()> {
        let process = self.process.read().await;
        process
            .send_command(PiCommand::NewSession {
                id: None,
                parent_session: None,
            })
            .await?;
        Ok(())
    }

    /// Set the current model.
    pub async fn set_model(&self, provider: &str, model_id: &str) -> Result<()> {
        let process = self.process.read().await;
        process
            .send_command(PiCommand::SetModel {
                id: None,
                provider: provider.to_string(),
                model_id: model_id.to_string(),
            })
            .await?;
        Ok(())
    }

    /// Get session statistics.
    pub async fn get_session_stats(&self) -> Result<SessionStats> {
        let process = self.process.read().await;
        let response = process
            .send_command(PiCommand::GetSessionStats { id: None })
            .await?;
        if !response.success {
            anyhow::bail!("get_session_stats failed: {:?}", response.error);
        }
        let data = response
            .data
            .context("get_session_stats returned no data")?;
        serde_json::from_value(data).context("failed to parse session stats")
    }

    /// Get available models.
    pub async fn get_available_models(&self) -> Result<Vec<crate::pi::PiModel>> {
        let process = self.process.read().await;
        let response = process
            .send_command(PiCommand::GetAvailableModels { id: None })
            .await?;
        if !response.success {
            anyhow::bail!("get_available_models failed: {:?}", response.error);
        }
        let data = response
            .data
            .context("get_available_models returned no data")?;
        let models = data.get("models").context("no models field in response")?;
        serde_json::from_value(models.clone()).context("failed to parse models")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn assistant_message() -> AgentMessage {
        AgentMessage {
            role: "assistant".to_string(),
            content: Value::Null,
            timestamp: None,
            api: None,
            provider: None,
            model: None,
            usage: None,
            stop_reason: None,
        }
    }

    #[test]
    fn test_stream_snapshot_replay_events() {
        let mut snapshot = StreamSnapshot::default();

        snapshot.apply_event(&PiEvent::AgentStart);
        snapshot.apply_event(&PiEvent::MessageStart {
            message: assistant_message(),
        });
        snapshot.apply_event(&PiEvent::MessageUpdate {
            message: assistant_message(),
            assistant_message_event: AssistantMessageEvent::TextDelta {
                content_index: 0,
                delta: "Hello".to_string(),
                partial: Value::Null,
            },
        });
        snapshot.apply_event(&PiEvent::MessageUpdate {
            message: assistant_message(),
            assistant_message_event: AssistantMessageEvent::ThinkingDelta {
                content_index: 0,
                delta: "Hmm".to_string(),
                partial: Value::Null,
            },
        });
        snapshot.apply_event(&PiEvent::MessageUpdate {
            message: assistant_message(),
            assistant_message_event: AssistantMessageEvent::ToolcallEnd {
                content_index: 0,
                tool_call: crate::pi::ToolCall {
                    id: "tool-1".to_string(),
                    name: "echo".to_string(),
                    arguments: json!({"value": 1}),
                },
                partial: Value::Null,
            },
        });
        snapshot.apply_event(&PiEvent::ToolExecutionEnd {
            tool_call_id: "tool-1".to_string(),
            tool_name: "echo".to_string(),
            result: crate::pi::ToolResult {
                content: vec![crate::pi::ContentBlock::Text {
                    text: "ok".to_string(),
                }],
                details: None,
            },
            is_error: false,
        });

        let events = snapshot.to_ws_events();
        assert_eq!(events.len(), 5);
        assert_eq!(events[0]["type"], "message_start");
        assert_eq!(events[1]["type"], "text");
        assert_eq!(events[2]["type"], "thinking");
        assert_eq!(events[3]["type"], "tool_use");
        assert_eq!(events[4]["type"], "tool_result");
    }

    #[test]
    fn test_pi_sessions_dir_escaping() {
        let service = MainChatPiService::new(
            PathBuf::from("/tmp/test"),
            true,
            MainChatPiServiceConfig::default(),
        );

        let work_dir = PathBuf::from("/home/user/.local/share/octo/users/main");
        let sessions_dir = service.get_pi_sessions_dir(&work_dir);

        // Should escape slashes and wrap with dashes
        assert!(
            sessions_dir
                .to_string_lossy()
                .contains("home-user-.local-share-octo-users-main")
        );
    }

    #[test]
    fn test_session_freshness_by_age() {
        let service = MainChatPiService::new(
            PathBuf::from("/tmp/test"),
            true,
            MainChatPiServiceConfig {
                max_session_age_hours: 1, // 1 hour for testing
                ..Default::default()
            },
        );

        // Fresh session (now)
        let fresh = LastSessionInfo {
            size: 1000,
            modified: SystemTime::now(),
        };
        assert!(service.should_continue_session(&fresh));

        // Stale session (2 hours ago)
        let stale = LastSessionInfo {
            size: 1000,
            modified: SystemTime::now() - Duration::from_secs(2 * 3600),
        };
        assert!(!service.should_continue_session(&stale));
    }

    #[test]
    fn test_session_freshness_by_size() {
        let service = MainChatPiService::new(
            PathBuf::from("/tmp/test"),
            true,
            MainChatPiServiceConfig {
                max_session_size_bytes: 1000, // 1KB for testing
                ..Default::default()
            },
        );

        // Small session
        let small = LastSessionInfo {
            size: 500,
            modified: SystemTime::now(),
        };
        assert!(service.should_continue_session(&small));

        // Large session
        let large = LastSessionInfo {
            size: 2000,
            modified: SystemTime::now(),
        };
        assert!(!service.should_continue_session(&large));
    }

    #[tokio::test]
    #[ignore] // Requires pi to be installed
    async fn test_pi_service_creation() {
        let temp = TempDir::new().unwrap();
        let main_dir = temp.path().join("main");
        std::fs::create_dir_all(&main_dir).unwrap();

        // Create minimal pi settings
        let pi_dir = main_dir.join(".pi");
        std::fs::create_dir_all(&pi_dir).unwrap();
        std::fs::write(
            pi_dir.join("settings.json"),
            r#"{"defaultProvider": "openai", "defaultModel": "gpt-4o-mini"}"#,
        )
        .unwrap();

        let service = MainChatPiService::new(
            temp.path().to_path_buf(),
            true,
            MainChatPiServiceConfig::default(),
        );

        // This would fail without pi installed
        let session = service.get_or_create_session("test").await;
        assert!(session.is_ok() || session.is_err()); // Just check it doesn't panic
    }
}
