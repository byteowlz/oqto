//! Pi Session Manager for octo-runner.
//!
//! Manages multiple Pi agent processes with:
//! - Process lifecycle (spawn, shutdown)
//! - Command routing (prompt, steer, follow_up, abort, compact)
//! - Event broadcasting to subscribers
//! - State tracking (Starting, Idle, Streaming, Compacting, Stopping)
//! - Idle session cleanup
//! - Persistence to hstry on AgentEnd

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{RwLock, broadcast, mpsc, oneshot};

use crate::local::SandboxConfig;
use crate::pi::{AgentMessage, PiCommand, PiEvent, PiMessage, PiResponse, PiState, SessionStats};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the Pi session manager.
#[derive(Debug, Clone)]
pub struct PiManagerConfig {
    /// Path to the Pi binary.
    pub pi_binary: PathBuf,
    /// Default working directory for sessions.
    pub default_cwd: PathBuf,
    /// Idle timeout before session cleanup (seconds).
    pub idle_timeout_secs: u64,
    /// Cleanup check interval (seconds).
    pub cleanup_interval_secs: u64,
    /// Path to hstry database (for direct writes).
    pub hstry_db_path: Option<PathBuf>,
    /// Sandbox configuration (if sandboxing is enabled).
    pub sandbox_config: Option<SandboxConfig>,
}

impl Default for PiManagerConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let data_dir = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(&home).join(".local").join("share"));

        Self {
            pi_binary: PathBuf::from(home.clone()).join(".bun/bin/pi"),
            default_cwd: PathBuf::from(&home).join("projects"),
            idle_timeout_secs: 300, // 5 minutes
            cleanup_interval_secs: 60,
            hstry_db_path: Some(data_dir.join("hstry").join("hstry.db")),
            sandbox_config: None,
        }
    }
}

// ============================================================================
// Session Configuration (per-session)
// ============================================================================

/// Configuration for creating a Pi session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionConfig {
    /// Working directory for Pi.
    pub cwd: PathBuf,
    /// Provider (anthropic, openai, etc.).
    #[serde(default)]
    pub provider: Option<String>,
    /// Model ID.
    #[serde(default)]
    pub model: Option<String>,
    /// Explicit session file to use (new or resume).
    #[serde(default)]
    pub session_file: Option<PathBuf>,
    /// Session file to continue from.
    #[serde(default)]
    pub continue_session: Option<PathBuf>,
    /// System prompt additions.
    #[serde(default)]
    pub system_prompt_files: Vec<PathBuf>,
    /// Environment variables.
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl Default for PiSessionConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        Self {
            cwd: PathBuf::from(home).join("projects"),
            provider: None,
            model: None,
            session_file: None,
            continue_session: None,
            system_prompt_files: Vec::new(),
            env: HashMap::new(),
        }
    }
}

// ============================================================================
// Session State
// ============================================================================

/// Session lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiSessionState {
    /// Session is starting up.
    Starting,
    /// Session is idle, waiting for commands.
    Idle,
    /// Session is actively streaming a response.
    Streaming,
    /// Session is compacting conversation history.
    Compacting,
    /// Session is shutting down.
    Stopping,
}

impl std::fmt::Display for PiSessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Starting => write!(f, "starting"),
            Self::Idle => write!(f, "idle"),
            Self::Streaming => write!(f, "streaming"),
            Self::Compacting => write!(f, "compacting"),
            Self::Stopping => write!(f, "stopping"),
        }
    }
}

/// Information about a Pi session (for external queries).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionInfo {
    /// Session ID.
    pub session_id: String,
    /// Current state.
    pub state: PiSessionState,
    /// Last activity timestamp (Unix ms).
    pub last_activity: i64,
    /// Number of active subscribers.
    pub subscriber_count: usize,
}

// ============================================================================
// Internal Session Command
// ============================================================================

/// Commands sent to a session's command loop.
/// Commands sent to a Pi session's command loop.
/// 
/// Note: Commands that need responses (GetState, GetMessages, etc.) don't include
/// oneshot senders here. Instead, response coordination happens via the shared
/// `pending_responses` map - the caller registers a waiter before sending the command,
/// and the reader task routes the response back.
#[derive(Debug)]
pub enum PiSessionCommand {
    // ========================================================================
    // Prompting
    // ========================================================================
    /// Send a user prompt.
    Prompt(String),
    /// Send a steering message (interrupt mid-run).
    Steer(String),
    /// Send a follow-up message (queue for after completion).
    FollowUp(String),
    /// Abort current operation.
    Abort,

    // ========================================================================
    // Session Management
    // ========================================================================
    /// Start a new session (optionally forking from parent).
    NewSession(Option<String>),
    /// Switch to a different session file.
    SwitchSession(String),
    /// Set session display name.
    SetSessionName(String),
    /// Export session to HTML.
    ExportHtml(Option<String>),

    // ========================================================================
    // State Queries (response via pending_responses)
    // ========================================================================
    /// Get current Pi state.
    GetState,
    /// Get all messages in the conversation.
    GetMessages,
    /// Get the last assistant message text.
    GetLastAssistantText,
    /// Get session statistics.
    GetSessionStats,
    /// Get available commands.
    GetCommands,

    // ========================================================================
    // Model Configuration
    // ========================================================================
    /// Set the model.
    SetModel { provider: String, model_id: String },
    /// Cycle to the next model.
    CycleModel,
    /// Get available models (response via pending_responses).
    GetAvailableModels,

    // ========================================================================
    // Thinking Configuration
    // ========================================================================
    /// Set thinking level.
    SetThinkingLevel(String),
    /// Cycle thinking level.
    CycleThinkingLevel,

    // ========================================================================
    // Queue Modes
    // ========================================================================
    /// Set steering message delivery mode.
    SetSteeringMode(String),
    /// Set follow-up message delivery mode.
    SetFollowUpMode(String),

    // ========================================================================
    // Compaction
    // ========================================================================
    /// Compact conversation history.
    Compact(Option<String>),
    /// Enable/disable auto-compaction.
    SetAutoCompaction(bool),

    // ========================================================================
    // Retry
    // ========================================================================
    /// Enable/disable auto-retry.
    SetAutoRetry(bool),
    /// Abort in-progress retry.
    AbortRetry,

    // ========================================================================
    // Forking (response via pending_responses)
    // ========================================================================
    /// Fork from a previous user message.
    Fork(String),
    /// Get messages available for forking.
    GetForkMessages,

    // ========================================================================
    // Bash Execution (response via pending_responses)
    // ========================================================================
    /// Execute a shell command.
    Bash(String),
    /// Abort running bash command.
    AbortBash,

    // ========================================================================
    // Extension UI
    // ========================================================================
    /// Respond to extension UI dialog.
    ExtensionUiResponse {
        id: String,
        value: Option<String>,
        confirmed: Option<bool>,
        cancelled: Option<bool>,
    },

    // ========================================================================
    // Lifecycle
    // ========================================================================
    /// Close the session.
    Close,
}

// ============================================================================
// Event Wrapper
// ============================================================================

/// Pi event wrapped with session context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiEventWrapper {
    /// Session ID this event belongs to.
    pub session_id: String,
    /// The actual event.
    pub event: PiEvent,
}

// ============================================================================
// Internal Session Structure
// ============================================================================

/// Pending response waiters - maps request ID to oneshot sender.
type PendingResponses = Arc<RwLock<HashMap<String, oneshot::Sender<PiResponse>>>>;

/// Internal session state (held by the manager).
struct PiSession {
    /// Session ID.
    id: String,
    /// Session configuration.
    #[allow(dead_code)]
    config: PiSessionConfig,
    /// Child process.
    process: Child,
    /// Current state.
    state: PiSessionState,
    /// Last activity timestamp.
    last_activity: Instant,
    /// Broadcast channel for events.
    event_tx: broadcast::Sender<PiEventWrapper>,
    /// Command sender to the session task.
    cmd_tx: mpsc::Sender<PiSessionCommand>,
    /// Pending response waiters (shared with reader task).
    pending_responses: PendingResponses,
    /// Handle to the background reader task.
    _reader_handle: tokio::task::JoinHandle<()>,
    /// Handle to the command processor task.
    _cmd_handle: tokio::task::JoinHandle<()>,
}

impl PiSession {
    fn subscriber_count(&self) -> usize {
        self.event_tx.receiver_count()
    }
}

// ============================================================================
// Pi Session Manager
// ============================================================================

/// Manager for Pi agent sessions.
///
/// Handles multiple concurrent Pi sessions with:
/// - Session lifecycle management
/// - Command routing
/// - Event broadcasting
/// - State tracking
/// - Idle cleanup
/// - Persistence to hstry
pub struct PiSessionManager {
    /// Active sessions.
    sessions: RwLock<HashMap<String, PiSession>>,
    /// Manager configuration.
    config: PiManagerConfig,
    /// Shutdown signal sender.
    shutdown_tx: broadcast::Sender<()>,
}

impl PiSessionManager {
    /// Create a new Pi session manager.
    pub fn new(config: PiManagerConfig) -> Arc<Self> {
        let (shutdown_tx, _) = broadcast::channel(1);
        Arc::new(Self {
            sessions: RwLock::new(HashMap::new()),
            config,
            shutdown_tx,
        })
    }

    /// Create a new session.
    pub async fn create_session(
        self: &Arc<Self>,
        session_id: String,
        config: PiSessionConfig,
    ) -> Result<()> {
        // Check if session already exists
        {
            let sessions = self.sessions.read().await;
            if sessions.contains_key(&session_id) {
                anyhow::bail!("Session '{}' already exists", session_id);
            }
        }

        info!("Creating Pi session '{}' in {:?}", session_id, config.cwd);

        // Build Pi arguments
        let mut pi_args: Vec<String> = vec!["--mode".to_string(), "rpc".to_string()];

        if let Some(ref provider) = config.provider {
            pi_args.push("--provider".to_string());
            pi_args.push(provider.clone());
        }
        if let Some(ref model) = config.model {
            pi_args.push("--model".to_string());
            pi_args.push(model.clone());
        }
        let session_file = config
            .session_file
            .as_ref()
            .or(config.continue_session.as_ref());
        if let Some(session_file) = session_file {
            pi_args.push("--session".to_string());
            pi_args.push(session_file.to_string_lossy().to_string());
        }
        for prompt_file in &config.system_prompt_files {
            pi_args.push("--system-prompt-file".to_string());
            pi_args.push(prompt_file.to_string_lossy().to_string());
        }

        // Build command - either direct or via bwrap sandbox
        let mut cmd = if let Some(ref sandbox_config) = self.config.sandbox_config {
            if sandbox_config.enabled {
                // Merge with workspace-specific config (can only add restrictions)
                let effective_config = sandbox_config.with_workspace_config(&config.cwd);

                // Build bwrap args for the workspace
                match effective_config.build_bwrap_args_for_user(&config.cwd, None) {
                    Some(bwrap_args) => {
                        // Command: bwrap [bwrap_args] -- pi [pi_args]
                        let mut cmd = Command::new("bwrap");

                        // Add bwrap args
                        for arg in &bwrap_args {
                            cmd.arg(arg);
                        }

                        // Add Pi binary and args
                        cmd.arg(&self.config.pi_binary);
                        for arg in &pi_args {
                            cmd.arg(arg);
                        }

                        info!(
                            "Sandboxing Pi session '{}' with profile '{}' ({} bwrap args)",
                            session_id,
                            effective_config.profile,
                            bwrap_args.len()
                        );
                        debug!(
                            "bwrap command: bwrap {} {} {:?}",
                            bwrap_args.join(" "),
                            self.config.pi_binary.display(),
                            pi_args
                        );

                        cmd
                    }
                    None => {
                        // SECURITY: bwrap not available but sandbox was requested
                        error!(
                            "SECURITY: Sandbox requested for Pi session '{}' but bwrap not available. \
                             Refusing to run unsandboxed.",
                            session_id
                        );
                        anyhow::bail!(
                            "Sandbox requested but bwrap not available. \
                             Install bubblewrap (bwrap) or disable sandboxing."
                        );
                    }
                }
            } else {
                // Sandbox config exists but is disabled
                let mut cmd = Command::new(&self.config.pi_binary);
                for arg in &pi_args {
                    cmd.arg(arg);
                }
                cmd.current_dir(&config.cwd);
                cmd
            }
        } else {
            // No sandbox config - run Pi directly
            let mut cmd = Command::new(&self.config.pi_binary);
            for arg in &pi_args {
                cmd.arg(arg);
            }
            cmd.current_dir(&config.cwd);
            cmd
        };

        // Set environment variables
        cmd.envs(&config.env);

        // Configure pipes
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Spawn the process
        let mut child = cmd.spawn().context("Failed to spawn Pi process")?;
        let pid = child.id().unwrap_or(0);
        info!(
            "Spawned Pi process for session '{}' (pid={})",
            session_id, pid
        );

        // Take ownership of pipes
        let stdin = child.stdin.take().context("Failed to get stdin")?;
        let stdout = child.stdout.take().context("Failed to get stdout")?;
        let stderr = child.stderr.take();

        // Create channels
        let (event_tx, _) = broadcast::channel::<PiEventWrapper>(256);
        let (cmd_tx, cmd_rx) = mpsc::channel::<PiSessionCommand>(32);

        // Shared state for the session
        let state = Arc::new(RwLock::new(PiSessionState::Starting));
        let last_activity = Arc::new(RwLock::new(Instant::now()));
        let pending_responses: PendingResponses = Arc::new(RwLock::new(HashMap::new()));

        // Spawn stdout reader task
        let reader_handle = {
            let session_id = session_id.clone();
            let event_tx = event_tx.clone();
            let state = Arc::clone(&state);
            let last_activity = Arc::clone(&last_activity);
            let hstry_db_path = self.config.hstry_db_path.clone();
            let work_dir = config.cwd.clone();
            let pending_responses = Arc::clone(&pending_responses);

            tokio::spawn(async move {
                Self::stdout_reader_task(
                    session_id,
                    stdout,
                    stderr,
                    event_tx,
                    state,
                    last_activity,
                    hstry_db_path,
                    work_dir,
                    pending_responses,
                )
                .await;
            })
        };

        // Spawn command processor task
        let cmd_handle = {
            let session_id = session_id.clone();
            let state = Arc::clone(&state);
            let last_activity = Arc::clone(&last_activity);

            tokio::spawn(async move {
                Self::command_processor_task(session_id, stdin, cmd_rx, state, last_activity).await;
            })
        };

        // Store the session (stdin is owned by the command processor task)
        let session = PiSession {
            id: session_id.clone(),
            config,
            process: child,
            state: PiSessionState::Starting,
            last_activity: Instant::now(),
            event_tx,
            cmd_tx,
            pending_responses,
            _reader_handle: reader_handle,
            _cmd_handle: cmd_handle,
        };

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), session);
        } // Write lock released here

        // Update state to Idle after initial setup
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(&session_id) {
            // Give Pi a moment to initialize before marking as Idle
            tokio::spawn({
                let session_id = session_id.clone();
                let cmd_tx = session.cmd_tx.clone();
                async move {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    // The state will be set to Idle by the reader when it receives events
                    debug!("Session '{}' initialization complete", session_id);
                    drop(cmd_tx);
                }
            });
        }

        info!("Session '{}' created successfully", session_id);
        Ok(())
    }

    /// Get or create a session.
    pub async fn get_or_create_session(
        self: &Arc<Self>,
        session_id: &str,
        config: PiSessionConfig,
    ) -> Result<()> {
        {
            let sessions = self.sessions.read().await;
            if sessions.contains_key(session_id) {
                debug!("Session '{}' already exists", session_id);
                return Ok(());
            }
        }
        self.create_session(session_id.to_string(), config).await
    }

    /// Send a prompt to a session.
    pub async fn prompt(&self, session_id: &str, message: &str) -> Result<()> {
        self.send_command(session_id, PiSessionCommand::Prompt(message.to_string()))
            .await
    }

    /// Send a steering message to a session.
    pub async fn steer(&self, session_id: &str, message: &str) -> Result<()> {
        self.send_command(session_id, PiSessionCommand::Steer(message.to_string()))
            .await
    }

    /// Send a follow-up message to a session.
    pub async fn follow_up(&self, session_id: &str, message: &str) -> Result<()> {
        self.send_command(session_id, PiSessionCommand::FollowUp(message.to_string()))
            .await
    }

    /// Abort current operation in a session.
    pub async fn abort(&self, session_id: &str) -> Result<()> {
        self.send_command(session_id, PiSessionCommand::Abort).await
    }

    /// Compact conversation history in a session.
    pub async fn compact(&self, session_id: &str, instructions: Option<&str>) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::Compact(instructions.map(String::from)),
        )
        .await
    }

    /// Subscribe to events from a session.
    pub async fn subscribe(&self, session_id: &str) -> Result<broadcast::Receiver<PiEventWrapper>> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;

        Ok(session.event_tx.subscribe())
    }

    /// List all sessions.
    pub async fn list_sessions(&self) -> Vec<PiSessionInfo> {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .map(|s| PiSessionInfo {
                session_id: s.id.clone(),
                state: s.state,
                last_activity: s.last_activity.elapsed().as_millis() as i64,
                subscriber_count: s.subscriber_count(),
            })
            .collect()
    }

    /// Get state of a specific session.
    pub async fn get_state(&self, session_id: &str) -> Result<PiState> {
        let request_id = "get_state".to_string();
        
        // Get session and register response waiter
        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (Arc::clone(&session.pending_responses), session.cmd_tx.clone())
        };

        // Register waiter before sending command
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        // Send the command
        cmd_tx
            .send(PiSessionCommand::GetState)
            .await
            .context("Failed to send GetState command")?;

        // Wait for response with timeout
        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for GetState response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "GetState failed: {}",
                response.error.unwrap_or_else(|| "unknown error".to_string())
            );
        }

        // Parse state from response data
        let data = response
            .data
            .ok_or_else(|| anyhow::anyhow!("GetState response missing data"))?;
        let state: PiState =
            serde_json::from_value(data).context("Failed to parse PiState from response")?;

        Ok(state)
    }

    /// Start a new session within the same Pi process.
    pub async fn new_session(&self, session_id: &str, parent_session: Option<&str>) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::NewSession(parent_session.map(String::from)),
        )
        .await
    }

    /// Get all messages from a session.
    pub async fn get_messages(&self, session_id: &str) -> Result<serde_json::Value> {
        let request_id = "get_messages".to_string();

        // Get session and register response waiter
        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (Arc::clone(&session.pending_responses), session.cmd_tx.clone())
        };

        // Register waiter before sending command
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        // Send the command
        cmd_tx
            .send(PiSessionCommand::GetMessages)
            .await
            .context("Failed to send GetMessages command")?;

        // Wait for response with timeout
        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for GetMessages response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "GetMessages failed: {}",
                response.error.unwrap_or_else(|| "unknown error".to_string())
            );
        }

        response
            .data
            .ok_or_else(|| anyhow::anyhow!("GetMessages response missing data"))
    }

    /// Set the model for a session.
    pub async fn set_model(&self, session_id: &str, provider: &str, model_id: &str) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::SetModel {
                provider: provider.to_string(),
                model_id: model_id.to_string(),
            },
        )
        .await
    }

    /// Get available models.
    pub async fn get_available_models(&self, session_id: &str) -> Result<serde_json::Value> {
        let request_id = "get_available_models".to_string();

        // Get session and register response waiter
        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (Arc::clone(&session.pending_responses), session.cmd_tx.clone())
        };

        // Register waiter before sending command
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        // Send the command
        cmd_tx
            .send(PiSessionCommand::GetAvailableModels)
            .await
            .context("Failed to send GetAvailableModels command")?;

        // Wait for response with timeout
        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for GetAvailableModels response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "GetAvailableModels failed: {}",
                response.error.unwrap_or_else(|| "unknown error".to_string())
            );
        }

        response
            .data
            .ok_or_else(|| anyhow::anyhow!("GetAvailableModels response missing data"))
    }

    /// Get session statistics.
    pub async fn get_session_stats(&self, session_id: &str) -> Result<SessionStats> {
        let request_id = "get_session_stats".to_string();

        // Get session and register response waiter
        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (Arc::clone(&session.pending_responses), session.cmd_tx.clone())
        };

        // Register waiter before sending command
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        // Send the command
        cmd_tx
            .send(PiSessionCommand::GetSessionStats)
            .await
            .context("Failed to send GetSessionStats command")?;

        // Wait for response with timeout
        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for GetSessionStats response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "GetSessionStats failed: {}",
                response.error.unwrap_or_else(|| "unknown error".to_string())
            );
        }

        let data = response
            .data
            .ok_or_else(|| anyhow::anyhow!("GetSessionStats response missing data"))?;
        let stats: SessionStats =
            serde_json::from_value(data).context("Failed to parse SessionStats from response")?;

        Ok(stats)
    }

    /// Switch to a different session file.
    pub async fn switch_session(&self, session_id: &str, session_path: &str) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::SwitchSession(session_path.to_string()),
        )
        .await
    }

    /// Set the display name for a session.
    pub async fn set_session_name(&self, session_id: &str, name: &str) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::SetSessionName(name.to_string()),
        )
        .await
    }

    /// Export session to HTML.
    pub async fn export_html(
        &self,
        session_id: &str,
        output_path: Option<&str>,
    ) -> Result<serde_json::Value> {
        let request_id = "export_html".to_string();

        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (Arc::clone(&session.pending_responses), session.cmd_tx.clone())
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        cmd_tx
            .send(PiSessionCommand::ExportHtml(output_path.map(String::from)))
            .await
            .context("Failed to send ExportHtml command")?;

        let response = tokio::time::timeout(Duration::from_secs(30), rx)
            .await
            .context("Timeout waiting for ExportHtml response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "ExportHtml failed: {}",
                response.error.unwrap_or_else(|| "unknown error".to_string())
            );
        }

        response
            .data
            .ok_or_else(|| anyhow::anyhow!("ExportHtml response missing data"))
    }

    /// Get the last assistant message text.
    pub async fn get_last_assistant_text(&self, session_id: &str) -> Result<Option<String>> {
        let request_id = "get_last_assistant_text".to_string();

        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (Arc::clone(&session.pending_responses), session.cmd_tx.clone())
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        cmd_tx
            .send(PiSessionCommand::GetLastAssistantText)
            .await
            .context("Failed to send GetLastAssistantText command")?;

        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for GetLastAssistantText response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "GetLastAssistantText failed: {}",
                response.error.unwrap_or_else(|| "unknown error".to_string())
            );
        }

        // Parse the text from response data
        Ok(response.data.and_then(|d| d.as_str().map(String::from)))
    }

    /// Get available commands.
    pub async fn get_commands(&self, session_id: &str) -> Result<serde_json::Value> {
        let request_id = "get_commands".to_string();

        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (Arc::clone(&session.pending_responses), session.cmd_tx.clone())
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        cmd_tx
            .send(PiSessionCommand::GetCommands)
            .await
            .context("Failed to send GetCommands command")?;

        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for GetCommands response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "GetCommands failed: {}",
                response.error.unwrap_or_else(|| "unknown error".to_string())
            );
        }

        response
            .data
            .ok_or_else(|| anyhow::anyhow!("GetCommands response missing data"))
    }

    /// Cycle to the next model.
    pub async fn cycle_model(&self, session_id: &str) -> Result<()> {
        self.send_command(session_id, PiSessionCommand::CycleModel)
            .await
    }

    /// Set the thinking level.
    pub async fn set_thinking_level(&self, session_id: &str, level: &str) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::SetThinkingLevel(level.to_string()),
        )
        .await
    }

    /// Cycle through thinking levels.
    pub async fn cycle_thinking_level(&self, session_id: &str) -> Result<()> {
        self.send_command(session_id, PiSessionCommand::CycleThinkingLevel)
            .await
    }

    /// Set steering message delivery mode.
    pub async fn set_steering_mode(&self, session_id: &str, mode: &str) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::SetSteeringMode(mode.to_string()),
        )
        .await
    }

    /// Set follow-up message delivery mode.
    pub async fn set_follow_up_mode(&self, session_id: &str, mode: &str) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::SetFollowUpMode(mode.to_string()),
        )
        .await
    }

    /// Enable/disable auto-compaction.
    pub async fn set_auto_compaction(&self, session_id: &str, enabled: bool) -> Result<()> {
        self.send_command(session_id, PiSessionCommand::SetAutoCompaction(enabled))
            .await
    }

    /// Enable/disable auto-retry.
    pub async fn set_auto_retry(&self, session_id: &str, enabled: bool) -> Result<()> {
        self.send_command(session_id, PiSessionCommand::SetAutoRetry(enabled))
            .await
    }

    /// Abort an in-progress retry.
    pub async fn abort_retry(&self, session_id: &str) -> Result<()> {
        self.send_command(session_id, PiSessionCommand::AbortRetry)
            .await
    }

    /// Fork from a previous user message.
    pub async fn fork(&self, session_id: &str, entry_id: &str) -> Result<serde_json::Value> {
        let request_id = "fork".to_string();

        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (Arc::clone(&session.pending_responses), session.cmd_tx.clone())
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        cmd_tx
            .send(PiSessionCommand::Fork(entry_id.to_string()))
            .await
            .context("Failed to send Fork command")?;

        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for Fork response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "Fork failed: {}",
                response.error.unwrap_or_else(|| "unknown error".to_string())
            );
        }

        response
            .data
            .ok_or_else(|| anyhow::anyhow!("Fork response missing data"))
    }

    /// Get messages available for forking.
    pub async fn get_fork_messages(&self, session_id: &str) -> Result<serde_json::Value> {
        let request_id = "get_fork_messages".to_string();

        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (Arc::clone(&session.pending_responses), session.cmd_tx.clone())
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        cmd_tx
            .send(PiSessionCommand::GetForkMessages)
            .await
            .context("Failed to send GetForkMessages command")?;

        let response = tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for GetForkMessages response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "GetForkMessages failed: {}",
                response.error.unwrap_or_else(|| "unknown error".to_string())
            );
        }

        response
            .data
            .ok_or_else(|| anyhow::anyhow!("GetForkMessages response missing data"))
    }

    /// Execute a bash command.
    pub async fn bash(&self, session_id: &str, command: &str) -> Result<serde_json::Value> {
        let request_id = "bash".to_string();

        let (pending_responses, cmd_tx) = {
            let sessions = self.sessions.read().await;
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;
            (Arc::clone(&session.pending_responses), session.cmd_tx.clone())
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = pending_responses.write().await;
            pending.insert(request_id.clone(), tx);
        }

        cmd_tx
            .send(PiSessionCommand::Bash(command.to_string()))
            .await
            .context("Failed to send Bash command")?;

        // Longer timeout for bash commands
        let response = tokio::time::timeout(Duration::from_secs(300), rx)
            .await
            .context("Timeout waiting for Bash response")?
            .context("Response channel closed")?;

        if !response.success {
            anyhow::bail!(
                "Bash failed: {}",
                response.error.unwrap_or_else(|| "unknown error".to_string())
            );
        }

        response
            .data
            .ok_or_else(|| anyhow::anyhow!("Bash response missing data"))
    }

    /// Abort a running bash command.
    pub async fn abort_bash(&self, session_id: &str) -> Result<()> {
        self.send_command(session_id, PiSessionCommand::AbortBash)
            .await
    }

    /// Respond to an extension UI prompt.
    pub async fn extension_ui_response(
        &self,
        session_id: &str,
        id: &str,
        value: Option<&str>,
        confirmed: Option<bool>,
        cancelled: Option<bool>,
    ) -> Result<()> {
        self.send_command(
            session_id,
            PiSessionCommand::ExtensionUiResponse {
                id: id.to_string(),
                value: value.map(String::from),
                confirmed,
                cancelled,
            },
        )
        .await
    }

    /// Close a session.
    pub async fn close_session(&self, session_id: &str) -> Result<()> {
        info!("Closing session '{}'", session_id);

        // Send close command
        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(session_id) {
                let _ = session.cmd_tx.send(PiSessionCommand::Close).await;
            }
        }

        // Remove from sessions map
        let mut sessions = self.sessions.write().await;
        if let Some(mut session) = sessions.remove(session_id) {
            // Kill the process if still running
            let _ = session.process.kill().await;
            info!("Session '{}' closed", session_id);
        }

        Ok(())
    }

    /// Run the idle cleanup loop.
    pub async fn cleanup_loop(self: Arc<Self>) {
        let interval = Duration::from_secs(self.config.cleanup_interval_secs);
        let idle_timeout = Duration::from_secs(self.config.idle_timeout_secs);

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                _ = tokio::time::sleep(interval) => {
                    self.cleanup_idle_sessions(idle_timeout).await;
                }
                _ = shutdown_rx.recv() => {
                    info!("Cleanup loop shutting down");
                    break;
                }
            }
        }
    }

    /// Shutdown all sessions and stop the manager.
    pub async fn shutdown(&self) {
        info!("Shutting down Pi session manager");

        // Signal shutdown
        let _ = self.shutdown_tx.send(());

        // Close all sessions
        let session_ids: Vec<String> = {
            let sessions = self.sessions.read().await;
            sessions.keys().cloned().collect()
        };

        for session_id in session_ids {
            if let Err(e) = self.close_session(&session_id).await {
                warn!("Error closing session '{}': {}", session_id, e);
            }
        }
    }

    // ========================================================================
    // Internal helpers
    // ========================================================================

    /// Send a command to a session.
    async fn send_command(&self, session_id: &str, cmd: PiSessionCommand) -> Result<()> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;

        session
            .cmd_tx
            .send(cmd)
            .await
            .context("Failed to send command to session")?;

        Ok(())
    }

    /// Cleanup idle sessions that have no subscribers.
    async fn cleanup_idle_sessions(&self, idle_timeout: Duration) {
        let now = Instant::now();
        let mut to_close = Vec::new();

        {
            let sessions = self.sessions.read().await;
            for (id, session) in sessions.iter() {
                let is_idle = session.state == PiSessionState::Idle;
                let no_subscribers = session.subscriber_count() == 0;
                let timed_out = now.duration_since(session.last_activity) > idle_timeout;

                if is_idle && no_subscribers && timed_out {
                    info!(
                        "Session '{}' idle for {:?} with no subscribers, marking for cleanup",
                        id,
                        now.duration_since(session.last_activity)
                    );
                    to_close.push(id.clone());
                }
            }
        }

        for session_id in to_close {
            if let Err(e) = self.close_session(&session_id).await {
                warn!(
                    "Error during idle cleanup of session '{}': {}",
                    session_id, e
                );
            }
        }
    }

    /// Background task that reads stdout and broadcasts events.
    async fn stdout_reader_task(
        session_id: String,
        stdout: tokio::process::ChildStdout,
        stderr: Option<tokio::process::ChildStderr>,
        event_tx: broadcast::Sender<PiEventWrapper>,
        state: Arc<RwLock<PiSessionState>>,
        last_activity: Arc<RwLock<Instant>>,
        hstry_db_path: Option<PathBuf>,
        work_dir: PathBuf,
        pending_responses: PendingResponses,
    ) {
        // Read stderr in a separate task (for debugging)
        if let Some(stderr) = stderr {
            let session_id = session_id.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if !line.trim().is_empty() {
                        debug!("Pi[{}] stderr: {}", session_id, line);
                    }
                }
            });
        }

        // Read stdout
        let mut reader = BufReader::new(stdout).lines();
        let mut pending_messages: Vec<AgentMessage> = Vec::new();

        // Mark as Idle after first successful read (Pi is ready)
        let mut first_event_seen = false;

        while let Ok(Some(line)) = reader.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            // Update last activity
            *last_activity.write().await = Instant::now();

            // Parse the message
            let msg = match PiMessage::parse(&line) {
                Ok(m) => m,
                Err(e) => {
                    warn!(
                        "Pi[{}] failed to parse message: {} - line: {}",
                        session_id, e, line
                    );
                    continue;
                }
            };

            // Handle responses vs events
            let event = match msg {
                PiMessage::Event(e) => e,
                PiMessage::Response(response) => {
                    debug!("Pi[{}] response: {:?}", session_id, response);
                    // Route response to waiting caller if there's a matching ID
                    if let Some(ref id) = response.id {
                        let mut pending = pending_responses.write().await;
                        if let Some(tx) = pending.remove(id) {
                            let _ = tx.send(response);
                        }
                    }
                    continue;
                }
            };

            // Update state based on event
            let new_state = match &event {
                PiEvent::AgentStart => {
                    debug!("Pi[{}] AgentStart", session_id);
                    Some(PiSessionState::Streaming)
                }
                PiEvent::AgentEnd { messages } => {
                    debug!(
                        "Pi[{}] AgentEnd with {} messages",
                        session_id,
                        messages.len()
                    );
                    pending_messages = messages.clone();
                    Some(PiSessionState::Idle)
                }
                PiEvent::AutoCompactionStart { .. } => {
                    debug!("Pi[{}] AutoCompactionStart", session_id);
                    Some(PiSessionState::Compacting)
                }
                PiEvent::AutoCompactionEnd { .. } => {
                    debug!("Pi[{}] AutoCompactionEnd", session_id);
                    Some(PiSessionState::Idle)
                }
                _ => None,
            };

            if let Some(new_state) = new_state {
                *state.write().await = new_state;
            }

            // Mark as ready after first event
            if !first_event_seen {
                first_event_seen = true;
                let current_state = *state.read().await;
                if current_state == PiSessionState::Starting {
                    *state.write().await = PiSessionState::Idle;
                }
            }

            // Broadcast the event
            let wrapped = PiEventWrapper {
                session_id: session_id.clone(),
                event: event.clone(),
            };
            let _ = event_tx.send(wrapped);

            // Persist to hstry on AgentEnd
            if matches!(event, PiEvent::AgentEnd { .. }) && !pending_messages.is_empty() {
                if let Some(ref db_path) = hstry_db_path {
                    if let Err(e) =
                        Self::persist_to_hstry(&session_id, &pending_messages, db_path, &work_dir)
                            .await
                    {
                        warn!("Pi[{}] failed to persist to hstry: {}", session_id, e);
                    } else {
                        debug!(
                            "Pi[{}] persisted {} messages to hstry",
                            session_id,
                            pending_messages.len()
                        );
                    }
                }
                pending_messages.clear();
            }
        }

        // Process exited
        info!("Pi[{}] stdout reader finished (process exited)", session_id);
        *state.write().await = PiSessionState::Stopping;
    }

    /// Background task that processes commands and writes to stdin.
    async fn command_processor_task(
        session_id: String,
        mut stdin: ChildStdin,
        mut cmd_rx: mpsc::Receiver<PiSessionCommand>,
        state: Arc<RwLock<PiSessionState>>,
        last_activity: Arc<RwLock<Instant>>,
    ) {
        while let Some(cmd) = cmd_rx.recv().await {
            let result = match cmd {
                PiSessionCommand::Prompt(msg) => {
                    let pi_cmd = PiCommand::Prompt {
                        id: None,
                        message: msg,
                        images: None,
                        streaming_behavior: None,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::Steer(msg) => {
                    let pi_cmd = PiCommand::Steer {
                        id: None,
                        message: msg,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::FollowUp(msg) => {
                    let pi_cmd = PiCommand::FollowUp {
                        id: None,
                        message: msg,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::Abort => {
                    let pi_cmd = PiCommand::Abort { id: None };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::Compact(instructions) => {
                    let pi_cmd = PiCommand::Compact {
                        id: None,
                        custom_instructions: instructions,
                    };
                    *state.write().await = PiSessionState::Compacting;
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::GetState => {
                    // Response coordination happens via pending_responses map
                    let pi_cmd = PiCommand::GetState {
                        id: Some("get_state".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::NewSession(parent) => {
                    let pi_cmd = PiCommand::NewSession {
                        id: Some("new_session".to_string()),
                        parent_session: parent,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::GetMessages => {
                    // Response coordination happens via pending_responses map
                    let pi_cmd = PiCommand::GetMessages {
                        id: Some("get_messages".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::SetModel { provider, model_id } => {
                    let pi_cmd = PiCommand::SetModel {
                        id: Some("set_model".to_string()),
                        provider,
                        model_id,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::GetAvailableModels => {
                    // Response coordination happens via pending_responses map
                    let pi_cmd = PiCommand::GetAvailableModels {
                        id: Some("get_available_models".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::GetSessionStats => {
                    let pi_cmd = PiCommand::GetSessionStats {
                        id: Some("get_session_stats".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Session management
                PiSessionCommand::SwitchSession(session_path) => {
                    let pi_cmd = PiCommand::SwitchSession {
                        id: Some("switch_session".to_string()),
                        session_path,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::SetSessionName(name) => {
                    let pi_cmd = PiCommand::SetSessionName {
                        id: Some("set_session_name".to_string()),
                        name,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::ExportHtml(output_path) => {
                    let pi_cmd = PiCommand::ExportHtml {
                        id: Some("export_html".to_string()),
                        output_path,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // State queries
                PiSessionCommand::GetLastAssistantText => {
                    let pi_cmd = PiCommand::GetLastAssistantText {
                        id: Some("get_last_assistant_text".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::GetCommands => {
                    let pi_cmd = PiCommand::GetCommands {
                        id: Some("get_commands".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Model configuration
                PiSessionCommand::CycleModel => {
                    let pi_cmd = PiCommand::CycleModel {
                        id: Some("cycle_model".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Thinking configuration
                PiSessionCommand::SetThinkingLevel(level) => {
                    let pi_cmd = PiCommand::SetThinkingLevel {
                        id: Some("set_thinking_level".to_string()),
                        level,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::CycleThinkingLevel => {
                    let pi_cmd = PiCommand::CycleThinkingLevel {
                        id: Some("cycle_thinking_level".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Queue modes
                PiSessionCommand::SetSteeringMode(mode) => {
                    let pi_cmd = PiCommand::SetSteeringMode {
                        id: Some("set_steering_mode".to_string()),
                        mode,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::SetFollowUpMode(mode) => {
                    let pi_cmd = PiCommand::SetFollowUpMode {
                        id: Some("set_follow_up_mode".to_string()),
                        mode,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Compaction
                PiSessionCommand::SetAutoCompaction(enabled) => {
                    let pi_cmd = PiCommand::SetAutoCompaction {
                        id: Some("set_auto_compaction".to_string()),
                        enabled,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Retry
                PiSessionCommand::SetAutoRetry(enabled) => {
                    let pi_cmd = PiCommand::SetAutoRetry {
                        id: Some("set_auto_retry".to_string()),
                        enabled,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::AbortRetry => {
                    let pi_cmd = PiCommand::AbortRetry {
                        id: Some("abort_retry".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Forking
                PiSessionCommand::Fork(entry_id) => {
                    let pi_cmd = PiCommand::Fork {
                        id: Some("fork".to_string()),
                        entry_id,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::GetForkMessages => {
                    let pi_cmd = PiCommand::GetForkMessages {
                        id: Some("get_fork_messages".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Bash execution
                PiSessionCommand::Bash(command) => {
                    let pi_cmd = PiCommand::Bash {
                        id: Some("bash".to_string()),
                        command,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                PiSessionCommand::AbortBash => {
                    let pi_cmd = PiCommand::AbortBash {
                        id: Some("abort_bash".to_string()),
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Extension UI
                PiSessionCommand::ExtensionUiResponse {
                    id,
                    value,
                    confirmed,
                    cancelled,
                } => {
                    let pi_cmd = PiCommand::ExtensionUiResponse {
                        id,
                        value,
                        confirmed,
                        cancelled,
                    };
                    Self::write_command(&mut stdin, &pi_cmd).await
                }
                // Lifecycle
                PiSessionCommand::Close => {
                    info!("Pi[{}] received Close command", session_id);
                    break;
                }
            };

            if let Err(e) = result {
                error!("Pi[{}] failed to write command: {}", session_id, e);
            }

            *last_activity.write().await = Instant::now();
        }

        info!("Pi[{}] command processor finished", session_id);
    }

    /// Write a command to Pi's stdin.
    async fn write_command(stdin: &mut ChildStdin, cmd: &PiCommand) -> Result<()> {
        let json = serde_json::to_string(cmd).context("Failed to serialize command")?;
        stdin
            .write_all(json.as_bytes())
            .await
            .context("Failed to write to stdin")?;
        stdin
            .write_all(b"\n")
            .await
            .context("Failed to write newline")?;
        stdin.flush().await.context("Failed to flush stdin")?;
        Ok(())
    }

    /// Persist messages to hstry database.
    async fn persist_to_hstry(
        session_id: &str,
        messages: &[AgentMessage],
        db_path: &PathBuf,
        work_dir: &PathBuf,
    ) -> Result<()> {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

        if !db_path.exists() {
            debug!(
                "hstry database not found at {:?}, skipping persistence",
                db_path
            );
            return Ok(());
        }

        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(false);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .context("Failed to connect to hstry database")?;

        // Get or create conversation
        let source_id = "pi";
        let external_id = session_id;
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        // Check if conversation exists
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT id FROM conversations WHERE source_id = ? AND external_id = ?")
                .bind(source_id)
                .bind(external_id)
                .fetch_optional(&pool)
                .await?;

        let metadata_json = serde_json::json!({
            "canonical_id": session_id,
            "readable_id": serde_json::Value::Null,
            "workdir": work_dir.to_string_lossy(),
        })
        .to_string();

        let conversation_id = if let Some((id,)) = existing {
            // Update timestamp
            sqlx::query("UPDATE conversations SET updated_at = ?, metadata_json = ? WHERE id = ?")
                .bind(now_secs)
                .bind(&metadata_json)
                .bind(&id)
                .execute(&pool)
                .await?;
            id
        } else {
            // Create new conversation
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO conversations (id, source_id, external_id, created_at, updated_at, metadata_json) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(source_id)
            .bind(external_id)
            .bind(now_secs)
            .bind(now_secs)
            .bind(&metadata_json)
            .execute(&pool)
            .await?;
            id
        };

        // Get current max index
        let max_idx: Option<i32> =
            sqlx::query_scalar("SELECT MAX(idx) FROM messages WHERE conversation_id = ?")
                .bind(&conversation_id)
                .fetch_one(&pool)
                .await?;

        let mut idx = max_idx.unwrap_or(-1) + 1;

        // Insert messages
        for msg in messages {
            let canon = crate::canon::pi_message_to_canon(msg, session_id);
            let parts_json =
                serde_json::to_string(&canon.parts).unwrap_or_else(|_| "[]".to_string());
            let metadata_json = canon
                .metadata
                .as_ref()
                .and_then(|m| serde_json::to_string(m).ok())
                .unwrap_or_default();
            let model = canon.model.as_ref().map(|m| m.full_id());
            let tokens = canon.tokens.as_ref().map(|t| t.total());
            let cost = canon.cost_usd;
            let created_at = Some(canon.created_at / 1000);

            let msg_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO messages (id, conversation_id, idx, role, content, parts_json, model, tokens, cost_usd, created_at, metadata) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&msg_id)
            .bind(&conversation_id)
            .bind(idx)
            .bind(canon.role.to_string())
            .bind(&canon.content)
            .bind(&parts_json)
            .bind(&model)
            .bind(tokens)
            .bind(cost)
            .bind(created_at)
            .bind(&metadata_json)
            .execute(&pool)
            .await?;

            idx += 1;
        }

        info!(
            "Persisted {} messages for session '{}' to hstry",
            messages.len(),
            session_id
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_state_display() {
        assert_eq!(PiSessionState::Starting.to_string(), "starting");
        assert_eq!(PiSessionState::Idle.to_string(), "idle");
        assert_eq!(PiSessionState::Streaming.to_string(), "streaming");
        assert_eq!(PiSessionState::Compacting.to_string(), "compacting");
        assert_eq!(PiSessionState::Stopping.to_string(), "stopping");
    }

    #[test]
    fn test_default_config() {
        let config = PiManagerConfig::default();
        assert!(config.pi_binary.to_string_lossy().contains(".bun/bin/pi"));
        assert_eq!(config.idle_timeout_secs, 300);
        assert_eq!(config.cleanup_interval_secs, 60);
    }

    #[test]
    fn test_session_config_default() {
        let config = PiSessionConfig::default();
        assert!(config.provider.is_none());
        assert!(config.model.is_none());
        assert!(config.continue_session.is_none());
        assert!(config.system_prompt_files.is_empty());
        assert!(config.env.is_empty());
    }

    #[test]
    fn test_session_state_serialization() {
        let state = PiSessionState::Streaming;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"streaming\"");

        let parsed: PiSessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, PiSessionState::Streaming);
    }
}
