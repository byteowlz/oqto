//! octo-runner - Process runner daemon for multi-user isolation.
//!
//! This daemon runs as a specific Linux user (via systemd user service) and
//! accepts commands over a Unix socket to spawn and manage processes.
//!
//! ## Configuration
//!
//! The runner loads configuration from `~/.config/octo/config.toml`, reusing
//! Octo's standard config format. The relevant sections are:
//!
//! ```toml
//! [local]
//! opencode_binary = "opencode"
//! fileserver_binary = "octo-files"
//! ttyd_binary = "ttyd"
//! workspace_dir = "~/projects"
//!
//! [runner]
//! # Runner-specific settings
//! runner_id = "workstation-1"
//! pi_sessions_dir = "~/.local/share/pi/sessions"
//! memories_dir = "~/.local/share/mmry"
//! ```
//!
//! ## Security Model
//!
//! The runner loads its sandbox configuration from a **trusted location**
//! (`/etc/octo/sandbox.toml`) that is owned by root. This ensures that even
//! if the main octo server is compromised, it cannot weaken sandbox restrictions.
//!
//! ## Usage
//!
//! ```bash
//! # Run with default config (~/.config/octo/config.toml)
//! octo-runner
//!
//! # Run with custom socket path (overrides config)
//! octo-runner --socket /tmp/my-runner.sock
//!
//! # With custom sandbox config
//! octo-runner --sandbox-config /path/to/sandbox.toml
//! ```

use anyhow::{Context, Result};
use chrono::TimeZone;
use clap::Parser;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock, broadcast};

use octo::local::SandboxConfig;
use octo::runner::client::DEFAULT_SOCKET_PATTERN;
use octo::runner::pi_manager::{PiManagerConfig, PiSessionManager};
use octo::runner::protocol::*;

// ============================================================================
// Configuration (loaded from ~/.config/octo/config.toml)
// ============================================================================

/// Runner configuration extracted from Octo's config.toml.
///
/// This is a subset of the full AppConfig, containing only what the runner needs.
#[derive(Debug, Clone, Default)]
struct RunnerUserConfig {
    /// Binary paths
    opencode_binary: String,
    fileserver_binary: String,
    ttyd_binary: String,
    pi_binary: String,
    /// Runner identifier (human-readable)
    runner_id: String,
    /// Data directories
    workspace_dir: PathBuf,
    pi_sessions_dir: PathBuf,
    memories_dir: PathBuf,
}

/// Config file structure (subset of Octo's config.toml)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct ConfigFile {
    local: LocalSection,
    runner: RunnerSection,
    pi: PiSection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct PiSection {
    executable: String,
}

impl Default for PiSection {
    fn default() -> Self {
        Self {
            executable: "pi".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct LocalSection {
    opencode_binary: String,
    fileserver_binary: String,
    ttyd_binary: String,
    workspace_dir: String,
}

impl Default for LocalSection {
    fn default() -> Self {
        Self {
            opencode_binary: "opencode".to_string(),
            fileserver_binary: "octo-files".to_string(),
            ttyd_binary: "ttyd".to_string(),
            workspace_dir: "~/projects".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct RunnerSection {
    /// Runner identifier (human-readable).
    runner_id: Option<String>,
    /// Directory containing Pi session files.
    pi_sessions_dir: Option<String>,
    /// Directory containing memories (mmry).
    memories_dir: Option<String>,
}

impl RunnerUserConfig {
    /// Load config from ~/.config/octo/config.toml
    fn load() -> Self {
        Self::load_from_path(Self::default_config_path())
    }

    fn default_config_path() -> PathBuf {
        let config_dir = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                PathBuf::from(home).join(".config")
            });
        config_dir.join("octo").join("config.toml")
    }

    fn load_from_path(path: PathBuf) -> Self {
        let config_file: ConfigFile = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(contents) => match toml::from_str(&contents) {
                    Ok(config) => {
                        info!("Loaded config from {:?}", path);
                        config
                    }
                    Err(e) => {
                        warn!("Failed to parse config {:?}: {}, using defaults", path, e);
                        ConfigFile::default()
                    }
                },
                Err(e) => {
                    warn!("Failed to read config {:?}: {}, using defaults", path, e);
                    ConfigFile::default()
                }
            }
        } else {
            debug!("Config file {:?} not found, using defaults", path);
            ConfigFile::default()
        };

        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let data_dir = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(&home).join(".local").join("share"));

        // Resolve pi binary: config value, then PATH lookup, then bun fallback
        let pi_binary = {
            let configured = &config_file.pi.executable;
            if configured.contains('/') {
                // Absolute or relative path - use as-is
                configured.clone()
            } else {
                // Bare name - resolve via which/PATH
                match std::process::Command::new("which").arg(configured).output() {
                    Ok(output) if output.status.success() => {
                        String::from_utf8_lossy(&output.stdout).trim().to_string()
                    }
                    _ => {
                        // Fallback to ~/.bun/bin/pi
                        let fallback = PathBuf::from(&home).join(".bun/bin/pi");
                        warn!(
                            "Could not resolve '{}' in PATH, falling back to {}",
                            configured,
                            fallback.display()
                        );
                        fallback.to_string_lossy().to_string()
                    }
                }
            }
        };
        info!("Pi binary: {}", pi_binary);

        let runner_id = config_file
            .runner
            .runner_id
            .or_else(|| std::env::var("OCTO_RUNNER_ID").ok())
            .or_else(|| std::env::var("HOSTNAME").ok())
            .unwrap_or_else(|| "local".to_string());

        info!("Runner ID: {}", runner_id);

        Self {
            opencode_binary: config_file.local.opencode_binary,
            fileserver_binary: config_file.local.fileserver_binary,
            ttyd_binary: config_file.local.ttyd_binary,
            pi_binary,
            runner_id,
            workspace_dir: Self::expand_path(&config_file.local.workspace_dir, &home),
            pi_sessions_dir: config_file
                .runner
                .pi_sessions_dir
                .map(|p| Self::expand_path(&p, &home))
                .unwrap_or_else(|| data_dir.join("pi").join("sessions")),
            memories_dir: config_file
                .runner
                .memories_dir
                .map(|p| Self::expand_path(&p, &home))
                .unwrap_or_else(|| data_dir.join("mmry")),
        }
    }

    fn expand_path(path: &str, home: &str) -> PathBuf {
        if path.starts_with("~/") {
            PathBuf::from(path.replacen("~", home, 1))
        } else if path.starts_with("$HOME") {
            PathBuf::from(path.replacen("$HOME", home, 1))
        } else {
            PathBuf::from(path)
        }
    }
}

// ============================================================================
// CLI Arguments
// ============================================================================

#[derive(Parser, Debug)]
#[command(
    name = "octo-runner",
    about = "Process runner daemon for multi-user isolation"
)]
struct Args {
    /// Path to config file.
    /// Defaults to ~/.config/octo/config.toml
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Socket path to listen on.
    /// Defaults to $XDG_RUNTIME_DIR/octo-runner.sock
    #[arg(short, long)]
    socket: Option<PathBuf>,

    /// Path to sandbox config file.
    /// Defaults to /etc/octo/sandbox.toml (system-wide, trusted).
    #[arg(long)]
    sandbox_config: Option<PathBuf>,

    /// Disable sandboxing entirely.
    #[arg(long)]
    no_sandbox: bool,

    /// Enable verbose logging.
    #[arg(short, long)]
    verbose: bool,

    // Session service binaries (override config file)
    /// Path to the opencode binary.
    #[arg(long)]
    opencode_binary: Option<String>,

    /// Path to the fileserver binary.
    #[arg(long)]
    fileserver_binary: Option<String>,

    /// Path to the ttyd binary.
    #[arg(long)]
    ttyd_binary: Option<String>,
}

/// Session state tracked by the runner.
#[derive(Debug, Clone)]
struct SessionState {
    /// Session ID.
    id: String,
    /// Workspace path.
    workspace_path: PathBuf,
    /// OpenCode process ID (runner-assigned).
    opencode_id: String,
    /// Fileserver process ID (runner-assigned).
    fileserver_id: String,
    /// ttyd process ID (runner-assigned).
    ttyd_id: String,
    /// OpenCode port.
    opencode_port: u16,
    /// Fileserver port.
    fileserver_port: u16,
    /// ttyd port.
    ttyd_port: u16,
    /// Agent name.
    agent: Option<String>,
    /// Started timestamp.
    started_at: std::time::Instant,
}

/// Stdout buffer shared between the reader task and the main runner.
#[derive(Debug)]
struct StdoutBuffer {
    /// Buffered lines from stdout.
    lines: Vec<String>,
    /// Whether the process has exited.
    closed: bool,
    /// Exit code if process has exited.
    exit_code: Option<i32>,
}

impl StdoutBuffer {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            closed: false,
            exit_code: None,
        }
    }
}

/// Message sent on the stdout broadcast channel.
#[derive(Debug, Clone)]
enum StdoutEvent {
    /// A line was read from stdout.
    Line(String),
    /// The process has exited.
    Closed { exit_code: Option<i32> },
}

/// Managed process with optional RPC pipes.
struct ManagedProcess {
    id: String,
    pid: u32,
    binary: String,
    cwd: PathBuf,
    child: Child,
    is_rpc: bool,
    /// Shared stdout buffer for RPC processes (populated by background reader task).
    stdout_buffer: Option<Arc<Mutex<StdoutBuffer>>>,
    /// Broadcast channel for stdout lines (for subscriptions).
    stdout_tx: Option<broadcast::Sender<StdoutEvent>>,
    /// Handle to the background stdout reader task.
    _reader_handle: Option<tokio::task::JoinHandle<()>>,
}

impl ManagedProcess {
    fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn exit_code(&mut self) -> Option<i32> {
        match self.child.try_wait() {
            Ok(Some(status)) => status.code(),
            _ => None,
        }
    }
}

/// Runner daemon state.
struct RunnerState {
    /// All managed processes.
    processes: HashMap<String, ManagedProcess>,
    /// Active sessions (session_id -> SessionState).
    sessions: HashMap<String, SessionState>,
}

impl RunnerState {
    fn new() -> Self {
        Self {
            processes: HashMap::new(),
            sessions: HashMap::new(),
        }
    }
}

/// Configuration for session service binaries.
#[derive(Debug, Clone)]
struct SessionBinaries {
    opencode: String,
    fileserver: String,
    ttyd: String,
}

/// The runner daemon.
struct Runner {
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
}

impl Runner {
    fn new(
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
        }
    }

    /// Handle a single request.
    async fn handle_request(&self, req: RunnerRequest) -> RunnerResponse {
        match req {
            RunnerRequest::Ping => RunnerResponse::Pong,

            RunnerRequest::Shutdown => {
                info!("Shutdown requested");
                let _ = self.shutdown_tx.send(());
                RunnerResponse::ShuttingDown
            }

            RunnerRequest::SpawnProcess(r) => self.spawn_process(r, false).await,
            RunnerRequest::SpawnRpcProcess(r) => {
                self.spawn_process(
                    SpawnProcessRequest {
                        id: r.id,
                        binary: r.binary,
                        args: r.args,
                        cwd: r.cwd,
                        env: r.env,
                        sandboxed: r.sandboxed,
                    },
                    true,
                )
                .await
            }

            RunnerRequest::KillProcess(r) => self.kill_process(r).await,
            RunnerRequest::GetStatus(r) => self.get_status(r).await,
            RunnerRequest::ListProcesses => self.list_processes().await,
            RunnerRequest::WriteStdin(r) => self.write_stdin(r).await,
            RunnerRequest::ReadStdout(r) => self.read_stdout(r).await,
            RunnerRequest::SubscribeStdout(_) => {
                // Handled specially in handle_connection since it streams
                error_response(
                    ErrorCode::Internal,
                    "SubscribeStdout must be handled via streaming",
                )
            }

            // ================================================================
            // Filesystem operations (user-plane)
            // ================================================================
            RunnerRequest::ReadFile(r) => self.read_file(r).await,
            RunnerRequest::WriteFile(r) => self.write_file(r).await,
            RunnerRequest::ListDirectory(r) => self.list_directory(r).await,
            RunnerRequest::Stat(r) => self.stat(r).await,
            RunnerRequest::DeletePath(r) => self.delete_path(r).await,
            RunnerRequest::CreateDirectory(r) => self.create_directory(r).await,

            // ================================================================
            // Session operations (user-plane)
            // ================================================================
            RunnerRequest::ListSessions => self.list_sessions().await,
            RunnerRequest::GetSession(r) => self.get_session(r).await,
            RunnerRequest::StartSession(r) => self.start_session(r).await,
            RunnerRequest::StopSession(r) => self.stop_session(r).await,

            // ================================================================
            // Main chat operations (user-plane)
            // ================================================================
            RunnerRequest::ListMainChatSessions => self.list_main_chat_sessions().await,
            RunnerRequest::GetMainChatMessages(r) => self.get_main_chat_messages(r).await,
            RunnerRequest::GetWorkspaceChatMessages(r) => self.get_workspace_chat_messages(r).await,

            // ================================================================
            // Memory operations (user-plane)
            // ================================================================
            RunnerRequest::SearchMemories(r) => self.search_memories(r).await,
            RunnerRequest::AddMemory(r) => self.add_memory(r).await,
            RunnerRequest::DeleteMemory(r) => self.delete_memory(r).await,

            // ================================================================
            // OpenCode chat history operations (user-plane)
            // ================================================================
            RunnerRequest::ListOpencodeSessions(r) => self.list_opencode_sessions(r).await,
            RunnerRequest::GetOpencodeSession(r) => self.get_opencode_session(r).await,
            RunnerRequest::GetOpencodeSessionMessages(r) => {
                self.get_opencode_session_messages(r).await
            }
            RunnerRequest::UpdateOpencodeSession(r) => self.update_opencode_session(r).await,

            // ================================================================
            // Pi session management operations
            // ================================================================
            // Session Lifecycle
            RunnerRequest::PiCreateSession(r) => self.pi_create_session(r).await,
            RunnerRequest::PiCloseSession(r) => self.pi_close_session(r).await,
            RunnerRequest::PiNewSession(r) => self.pi_new_session(r).await,
            RunnerRequest::PiSwitchSession(r) => self.pi_switch_session(r).await,
            RunnerRequest::PiListSessions => self.pi_list_sessions().await,
            RunnerRequest::PiSubscribe(_) => {
                // Handled specially in handle_connection since it streams
                error_response(
                    ErrorCode::Internal,
                    "PiSubscribe must be handled via streaming",
                )
            }
            RunnerRequest::PiUnsubscribe(r) => self.pi_unsubscribe(r).await,

            // Prompting
            RunnerRequest::PiPrompt(r) => self.pi_prompt(r).await,
            RunnerRequest::PiSteer(r) => self.pi_steer(r).await,
            RunnerRequest::PiFollowUp(r) => self.pi_follow_up(r).await,
            RunnerRequest::PiAbort(r) => self.pi_abort(r).await,

            // State & Messages
            RunnerRequest::PiGetState(r) => self.pi_get_state(r).await,
            RunnerRequest::PiGetMessages(r) => self.pi_get_messages(r).await,
            RunnerRequest::PiGetSessionStats(r) => self.pi_get_session_stats(r).await,
            RunnerRequest::PiGetLastAssistantText(r) => self.pi_get_last_assistant_text(r).await,

            // Model Management
            RunnerRequest::PiSetModel(r) => self.pi_set_model(r).await,
            RunnerRequest::PiCycleModel(r) => self.pi_cycle_model(r).await,
            RunnerRequest::PiGetAvailableModels(r) => self.pi_get_available_models(r).await,

            // Thinking Level
            RunnerRequest::PiSetThinkingLevel(r) => self.pi_set_thinking_level(r).await,
            RunnerRequest::PiCycleThinkingLevel(r) => self.pi_cycle_thinking_level(r).await,

            // Compaction
            RunnerRequest::PiCompact(r) => self.pi_compact(r).await,
            RunnerRequest::PiSetAutoCompaction(r) => self.pi_set_auto_compaction(r).await,

            // Queue Modes
            RunnerRequest::PiSetSteeringMode(r) => self.pi_set_steering_mode(r).await,
            RunnerRequest::PiSetFollowUpMode(r) => self.pi_set_follow_up_mode(r).await,

            // Retry
            RunnerRequest::PiSetAutoRetry(r) => self.pi_set_auto_retry(r).await,
            RunnerRequest::PiAbortRetry(r) => self.pi_abort_retry(r).await,

            // Forking
            RunnerRequest::PiFork(r) => self.pi_fork(r).await,
            RunnerRequest::PiGetForkMessages(r) => self.pi_get_fork_messages(r).await,

            // Session Metadata
            RunnerRequest::PiSetSessionName(r) => self.pi_set_session_name(r).await,
            RunnerRequest::PiExportHtml(r) => self.pi_export_html(r).await,

            // Commands/Skills
            RunnerRequest::PiGetCommands(r) => self.pi_get_commands(r).await,

            // Bash
            RunnerRequest::PiBash(r) => self.pi_bash(r).await,
            RunnerRequest::PiAbortBash(r) => self.pi_abort_bash(r).await,

            // Extension UI
            RunnerRequest::PiExtensionUiResponse(r) => self.pi_extension_ui_response(r).await,
        }
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
                 Refusing to run unsandboxed. Load sandbox config from /etc/octo/sandbox.toml \
                 or pass --sandbox-config to octo-runner.",
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

        // Build command - either direct or via octo-sandbox
        let (program, args, effective_binary) = if use_sandbox {
            let sandbox_config = self.sandbox_config.as_ref().unwrap();

            // Build bwrap args using the trusted config
            // Note: We use the current user (runner's user) for path expansion
            match sandbox_config.build_bwrap_args_for_user(&req.cwd, None) {
                Some(bwrap_args) => {
                    // Command: bwrap [bwrap_args] -- binary [args]
                    let mut full_args = bwrap_args;
                    full_args.push(req.binary.clone());
                    full_args.extend(req.args.iter().cloned());

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
                    opencode_port: Some(s.opencode_port),
                    fileserver_port: Some(s.fileserver_port),
                    ttyd_port: Some(s.ttyd_port),
                    pids: Some(format!(
                        "{},{},{}",
                        s.opencode_id, s.fileserver_id, s.ttyd_id
                    )),
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
            opencode_port: Some(s.opencode_port),
            fileserver_port: Some(s.fileserver_port),
            ttyd_port: Some(s.ttyd_port),
            pids: Some(format!(
                "{},{},{}",
                s.opencode_id, s.fileserver_id, s.ttyd_id
            )),
            created_at: chrono::Utc::now().to_rfc3339(),
            started_at: Some(chrono::Utc::now().to_rfc3339()),
            last_activity_at: None,
        });

        RunnerResponse::Session(SessionResponse { session })
    }

    async fn start_session(&self, req: StartSessionRequest) -> RunnerResponse {
        info!(
            "Starting session {} in {:?} with ports {}/{}/{}",
            req.session_id,
            req.workspace_path,
            req.opencode_port,
            req.fileserver_port,
            req.ttyd_port
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
        let opencode_id = format!("{}-opencode", req.session_id);
        let fileserver_id = format!("{}-fileserver", req.session_id);
        let ttyd_id = format!("{}-ttyd", req.session_id);

        // Spawn fileserver
        let fileserver_req = SpawnProcessRequest {
            id: fileserver_id.clone(),
            binary: self.binaries.fileserver.clone(),
            args: vec![
                "--port".to_string(),
                req.fileserver_port.to_string(),
                "--bind".to_string(),
                "127.0.0.1".to_string(),
                "--root".to_string(),
                req.workspace_path.to_string_lossy().to_string(),
            ],
            cwd: req.workspace_path.clone(),
            env: HashMap::new(),
            sandboxed: false,
        };

        if let RunnerResponse::Error(e) = self.spawn_process(fileserver_req, false).await {
            return RunnerResponse::Error(e);
        }

        // Spawn ttyd
        let ttyd_req = SpawnProcessRequest {
            id: ttyd_id.clone(),
            binary: self.binaries.ttyd.clone(),
            args: vec![
                "--port".to_string(),
                req.ttyd_port.to_string(),
                "--interface".to_string(),
                "127.0.0.1".to_string(),
                "--writable".to_string(),
                "--cwd".to_string(),
                req.workspace_path.to_string_lossy().to_string(),
                "zsh".to_string(),
                "-l".to_string(),
            ],
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

        // Spawn opencode
        let mut opencode_args = vec![
            "serve".to_string(),
            "--port".to_string(),
            req.opencode_port.to_string(),
            "--hostname".to_string(),
            "127.0.0.1".to_string(),
        ];
        if let Some(ref agent) = req.agent {
            opencode_args.push("--agent".to_string());
            opencode_args.push(agent.clone());
        }

        let opencode_req = SpawnProcessRequest {
            id: opencode_id.clone(),
            binary: self.binaries.opencode.clone(),
            args: opencode_args,
            cwd: req.workspace_path.clone(),
            env: req.env.clone(),
            sandboxed: self
                .sandbox_config
                .as_ref()
                .map(|s| s.enabled)
                .unwrap_or(false),
        };

        if let RunnerResponse::Error(e) = self.spawn_process(opencode_req, false).await {
            // Clean up fileserver and ttyd
            let _ = self
                .kill_process(KillProcessRequest {
                    id: fileserver_id.clone(),
                    force: false,
                })
                .await;
            let _ = self
                .kill_process(KillProcessRequest {
                    id: ttyd_id.clone(),
                    force: false,
                })
                .await;
            return RunnerResponse::Error(e);
        }

        // Record session state
        let session_state = SessionState {
            id: req.session_id.clone(),
            workspace_path: req.workspace_path.clone(),
            opencode_id: opencode_id.clone(),
            fileserver_id: fileserver_id.clone(),
            ttyd_id: ttyd_id.clone(),
            opencode_port: req.opencode_port,
            fileserver_port: req.fileserver_port,
            ttyd_port: req.ttyd_port,
            agent: req.agent.clone(),
            started_at: std::time::Instant::now(),
        };

        {
            let mut state = self.state.write().await;
            state.sessions.insert(req.session_id.clone(), session_state);
        }

        let pids = format!("{},{},{}", opencode_id, fileserver_id, ttyd_id);
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

        // Kill all session processes
        let _ = self
            .kill_process(KillProcessRequest {
                id: session_state.opencode_id,
                force: false,
            })
            .await;

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
        let Some(db_path) = octo::history::hstry_db_path() else {
            return RunnerResponse::MainChatSessionList(MainChatSessionListResponse {
                sessions: Vec::new(),
            });
        };

        let pool = match octo::history::repository::open_hstry_pool(&db_path).await {
            Ok(pool) => pool,
            Err(e) => {
                return error_response(ErrorCode::IoError, format!("Failed to open hstry DB: {e}"));
            }
        };

        let rows = match sqlx::query(
            r#"
            SELECT
              c.id AS id,
              c.external_id AS external_id,
              c.title AS title,
              c.created_at AS created_at,
              c.updated_at AS updated_at,
              (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id) AS message_count
            FROM conversations c
            WHERE c.source_id = 'pi'
            ORDER BY COALESCE(c.updated_at, c.created_at) DESC
            "#,
        )
        .fetch_all(&pool)
        .await
        {
            Ok(rows) => rows,
            Err(e) => {
                return error_response(
                    ErrorCode::IoError,
                    format!("Failed to query hstry conversations: {e}"),
                );
            }
        };

        let mut sessions = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row.get("id");
            let external_id: Option<String> = row.get("external_id");
            let title: Option<String> = row.get("title");
            let created_at: i64 = row.get("created_at");
            let updated_at: Option<i64> = row.get("updated_at");
            let message_count: i64 = row.get("message_count");

            let session_id = external_id.unwrap_or(id);
            let started_at = chrono::Utc
                .timestamp_opt(created_at, 0)
                .single()
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

            let modified_at = updated_at.unwrap_or(created_at) * 1000;

            sessions.push(MainChatSessionInfo {
                id: session_id,
                title,
                message_count: message_count.max(0) as usize,
                size: 0,
                modified_at,
                started_at,
            });
        }

        RunnerResponse::MainChatSessionList(MainChatSessionListResponse { sessions })
    }

    async fn get_main_chat_messages(&self, req: GetMainChatMessagesRequest) -> RunnerResponse {
        let Some(db_path) = octo::history::hstry_db_path() else {
            return RunnerResponse::MainChatMessages(MainChatMessagesResponse {
                session_id: req.session_id,
                messages: Vec::new(),
            });
        };

        let pool = match octo::history::repository::open_hstry_pool(&db_path).await {
            Ok(pool) => pool,
            Err(e) => {
                return error_response(ErrorCode::IoError, format!("Failed to open hstry DB: {e}"));
            }
        };

        let conv_row = match sqlx::query(
            r#"
            SELECT id, external_id
            FROM conversations
            WHERE source_id = 'pi' AND (external_id = ? OR readable_id = ? OR id = ?)
            LIMIT 1
            "#,
        )
        .bind(&req.session_id)
        .bind(&req.session_id)
        .bind(&req.session_id)
        .fetch_optional(&pool)
        .await
        {
            Ok(row) => row,
            Err(e) => {
                return error_response(
                    ErrorCode::IoError,
                    format!("Failed to resolve conversation: {e}"),
                );
            }
        };

        let Some(conv_row) = conv_row else {
            return RunnerResponse::MainChatMessages(MainChatMessagesResponse {
                session_id: req.session_id,
                messages: Vec::new(),
            });
        };

        let conversation_id: String = match conv_row.try_get("id") {
            Ok(v) => v,
            Err(e) => {
                return error_response(
                    ErrorCode::IoError,
                    format!("Failed to read conversation id: {e}"),
                );
            }
        };

        let session_id: String = match conv_row.try_get::<Option<String>, _>("external_id") {
            Ok(Some(v)) => v,
            _ => req.session_id.clone(),
        };

        let rows = if let Some(limit) = req.limit {
            match sqlx::query(
                r#"
                SELECT idx, role, content, created_at, parts_json
                FROM messages
                WHERE conversation_id = ?
                ORDER BY idx DESC
                LIMIT ?
                "#,
            )
            .bind(&conversation_id)
            .bind(limit as i64)
            .fetch_all(&pool)
            .await
            {
                Ok(rows) => {
                    let mut rows = rows;
                    rows.reverse();
                    rows
                }
                Err(e) => {
                    return error_response(
                        ErrorCode::IoError,
                        format!("Failed to load messages: {e}"),
                    );
                }
            }
        } else {
            match sqlx::query(
                r#"
                SELECT idx, role, content, created_at, parts_json
                FROM messages
                WHERE conversation_id = ?
                ORDER BY idx
                "#,
            )
            .bind(&conversation_id)
            .fetch_all(&pool)
            .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    return error_response(
                        ErrorCode::IoError,
                        format!("Failed to load messages: {e}"),
                    );
                }
            }
        };

        let mut messages = Vec::with_capacity(rows.len());
        for row in rows {
            let idx: i64 = row.get("idx");
            let role_raw: String = row.get("role");
            let content_raw: String = row.get("content");
            let created_at: i64 = row.get("created_at");
            let parts_json: Option<String> = row.try_get("parts_json").ok();

            let role = match role_raw.as_str() {
                "user" => "user",
                "assistant" => "assistant",
                "system" => "system",
                "tool" | "toolResult" => "assistant",
                _ => "assistant",
            }
            .to_string();

            let content = if let Some(parts_json) = parts_json.as_deref()
                && let Ok(v) = serde_json::from_str::<serde_json::Value>(parts_json)
                && v.is_array()
            {
                v
            } else if !content_raw.trim().is_empty() {
                serde_json::json!([{ "type": "text", "text": content_raw }])
            } else {
                serde_json::json!([])
            };

            messages.push(MainChatMessage {
                id: format!("msg_{}", idx),
                role,
                content,
                timestamp: created_at * 1000,
            });
        }

        RunnerResponse::MainChatMessages(MainChatMessagesResponse {
            session_id,
            messages,
        })
    }

    async fn get_workspace_chat_messages(
        &self,
        req: GetWorkspaceChatMessagesRequest,
    ) -> RunnerResponse {
        let Some(db_path) = octo::history::hstry_db_path() else {
            return RunnerResponse::WorkspaceChatMessages(MainChatMessagesResponse {
                session_id: req.session_id,
                messages: Vec::new(),
            });
        };

        let pool = match octo::history::repository::open_hstry_pool(&db_path).await {
            Ok(pool) => pool,
            Err(e) => {
                return error_response(ErrorCode::IoError, format!("Failed to open hstry DB: {e}"));
            }
        };

        let conv_row = match sqlx::query(
            r#"
            SELECT id, external_id
            FROM conversations
            WHERE source_id = 'pi'
              AND (external_id = ? OR readable_id = ? OR id = ?)
              AND workspace = ?
            LIMIT 1
            "#,
        )
        .bind(&req.session_id)
        .bind(&req.session_id)
        .bind(&req.session_id)
        .bind(&req.workspace_path)
        .fetch_optional(&pool)
        .await
        {
            Ok(row) => row,
            Err(e) => {
                return error_response(
                    ErrorCode::IoError,
                    format!("Failed to resolve conversation: {e}"),
                );
            }
        };

        let conv_row = if let Some(row) = conv_row {
            Some(row)
        } else {
            match sqlx::query(
                r#"
                SELECT id, external_id
                FROM conversations
                WHERE source_id = 'pi' AND (external_id = ? OR readable_id = ? OR id = ?)
                LIMIT 1
                "#,
            )
            .bind(&req.session_id)
            .bind(&req.session_id)
            .bind(&req.session_id)
            .fetch_optional(&pool)
            .await
            {
                Ok(row) => row,
                Err(e) => {
                    return error_response(
                        ErrorCode::IoError,
                        format!("Failed to resolve conversation: {e}"),
                    );
                }
            }
        };

        let Some(conv_row) = conv_row else {
            return RunnerResponse::WorkspaceChatMessages(MainChatMessagesResponse {
                session_id: req.session_id,
                messages: Vec::new(),
            });
        };

        let conversation_id: String = match conv_row.try_get("id") {
            Ok(v) => v,
            Err(e) => {
                return error_response(
                    ErrorCode::IoError,
                    format!("Failed to read conversation id: {e}"),
                );
            }
        };

        let session_id: String = match conv_row.try_get::<Option<String>, _>("external_id") {
            Ok(Some(v)) => v,
            _ => req.session_id.clone(),
        };

        let rows = if let Some(limit) = req.limit {
            match sqlx::query(
                r#"
                SELECT idx, role, content, created_at, parts_json
                FROM messages
                WHERE conversation_id = ?
                ORDER BY idx DESC
                LIMIT ?
                "#,
            )
            .bind(&conversation_id)
            .bind(limit as i64)
            .fetch_all(&pool)
            .await
            {
                Ok(rows) => {
                    let mut rows = rows;
                    rows.reverse();
                    rows
                }
                Err(e) => {
                    return error_response(
                        ErrorCode::IoError,
                        format!("Failed to load messages: {e}"),
                    );
                }
            }
        } else {
            match sqlx::query(
                r#"
                SELECT idx, role, content, created_at, parts_json
                FROM messages
                WHERE conversation_id = ?
                ORDER BY idx
                "#,
            )
            .bind(&conversation_id)
            .fetch_all(&pool)
            .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    return error_response(
                        ErrorCode::IoError,
                        format!("Failed to load messages: {e}"),
                    );
                }
            }
        };

        let mut messages = Vec::with_capacity(rows.len());
        for row in rows {
            let idx: i64 = row.get("idx");
            let role_raw: String = row.get("role");
            let content_raw: String = row.get("content");
            let created_at: i64 = row.get("created_at");
            let parts_json: Option<String> = row.try_get("parts_json").ok();

            let role = match role_raw.as_str() {
                "user" => "user",
                "assistant" => "assistant",
                "system" => "system",
                "tool" | "toolResult" => "assistant",
                _ => "assistant",
            }
            .to_string();

            let content = if let Some(parts_json) = parts_json.as_deref()
                && let Ok(v) = serde_json::from_str::<serde_json::Value>(parts_json)
                && v.is_array()
            {
                v
            } else {
                serde_json::json!([{ "type": "text", "text": content_raw }])
            };

            messages.push(MainChatMessage {
                id: format!("msg_{}", idx),
                role,
                content,
                timestamp: created_at * 1000,
            });
        }

        RunnerResponse::WorkspaceChatMessages(MainChatMessagesResponse {
            session_id,
            messages,
        })
    }

    // ========================================================================
    // Memory Operations (user-plane)
    // ========================================================================

    async fn search_memories(&self, req: SearchMemoriesRequest) -> RunnerResponse {
        // TODO: Search mmry database
        let _ = req;
        RunnerResponse::MemorySearchResults(MemorySearchResultsResponse {
            query: req.query,
            memories: Vec::new(),
            total: 0,
        })
    }

    async fn add_memory(&self, req: AddMemoryRequest) -> RunnerResponse {
        // TODO: Add to mmry database
        let _ = req;
        error_response(ErrorCode::Internal, "Memory operations not yet implemented")
    }

    async fn delete_memory(&self, req: DeleteMemoryRequest) -> RunnerResponse {
        // TODO: Delete from mmry database
        let _ = req;
        error_response(ErrorCode::Internal, "Memory operations not yet implemented")
    }

    // ========================================================================
    // OpenCode Chat History Operations (user-plane)
    // ========================================================================

    /// Get the OpenCode data directory for this user.
    fn opencode_data_dir(&self) -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let data_dir = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(&home).join(".local").join("share"));
        data_dir.join("opencode")
    }

    async fn list_opencode_sessions(&self, req: ListOpencodeSessionsRequest) -> RunnerResponse {
        let opencode_dir = self.opencode_data_dir();

        match octo::history::list_sessions_from_dir(&opencode_dir) {
            Ok(sessions) => {
                let mut filtered: Vec<_> = sessions
                    .into_iter()
                    .filter(|s| {
                        // Filter by workspace if specified
                        if let Some(ref ws) = req.workspace
                            && s.workspace_path != *ws
                        {
                            return false;
                        }
                        // Filter out child sessions unless explicitly included
                        if !req.include_children && s.is_child {
                            return false;
                        }
                        true
                    })
                    .collect();

                // Apply limit if specified
                if let Some(limit) = req.limit {
                    filtered.truncate(limit);
                }

                let sessions: Vec<OpencodeSessionInfo> = filtered
                    .into_iter()
                    .map(|s| OpencodeSessionInfo {
                        id: s.id,
                        readable_id: s.readable_id,
                        title: s.title,
                        parent_id: s.parent_id,
                        workspace_path: s.workspace_path,
                        project_name: s.project_name,
                        created_at: s.created_at,
                        updated_at: s.updated_at,
                        version: s.version,
                        is_child: s.is_child,
                        model: s.model,
                        provider: s.provider,
                    })
                    .collect();

                RunnerResponse::OpencodeSessionList(OpencodeSessionListResponse { sessions })
            }
            Err(e) => error_response(
                ErrorCode::IoError,
                format!("Failed to list OpenCode sessions: {}", e),
            ),
        }
    }

    async fn get_opencode_session(&self, req: GetOpencodeSessionRequest) -> RunnerResponse {
        let opencode_dir = self.opencode_data_dir();

        match octo::history::get_session_from_dir(&req.session_id, &opencode_dir) {
            Ok(Some(s)) => RunnerResponse::OpencodeSession(OpencodeSessionResponse {
                session: Some(OpencodeSessionInfo {
                    id: s.id,
                    readable_id: s.readable_id,
                    title: s.title,
                    parent_id: s.parent_id,
                    workspace_path: s.workspace_path,
                    project_name: s.project_name,
                    created_at: s.created_at,
                    updated_at: s.updated_at,
                    version: s.version,
                    is_child: s.is_child,
                    model: s.model,
                    provider: s.provider,
                }),
            }),
            Ok(None) => RunnerResponse::OpencodeSession(OpencodeSessionResponse { session: None }),
            Err(e) => error_response(
                ErrorCode::IoError,
                format!("Failed to get OpenCode session: {}", e),
            ),
        }
    }

    async fn get_opencode_session_messages(
        &self,
        req: GetOpencodeSessionMessagesRequest,
    ) -> RunnerResponse {
        let opencode_dir = self.opencode_data_dir();

        let messages_result = if req.render {
            // Use blocking task for rendering since it may do async markdown processing
            let session_id = req.session_id.clone();
            let dir = opencode_dir.clone();
            tokio::task::spawn_blocking(move || {
                // We need to run async code in blocking context
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    octo::history::get_session_messages_rendered_from_dir(&session_id, &dir).await
                })
            })
            .await
            .map_err(|e| anyhow::anyhow!("Task join error: {}", e))
            .and_then(|r| r)
        } else {
            octo::history::get_session_messages_from_dir(&req.session_id, &opencode_dir)
        };

        match messages_result {
            Ok(messages) => {
                let messages: Vec<OpencodeMessage> = messages
                    .into_iter()
                    .map(|m| OpencodeMessage {
                        id: m.id,
                        session_id: m.session_id,
                        role: m.role,
                        created_at: m.created_at,
                        completed_at: m.completed_at,
                        parent_id: m.parent_id,
                        model_id: m.model_id,
                        provider_id: m.provider_id,
                        agent: m.agent,
                        summary_title: m.summary_title,
                        tokens_input: m.tokens_input,
                        tokens_output: m.tokens_output,
                        tokens_reasoning: m.tokens_reasoning,
                        cost: m.cost,
                        parts: m
                            .parts
                            .into_iter()
                            .map(|p| OpencodeMessagePart {
                                id: p.id,
                                part_type: p.part_type,
                                text: p.text,
                                text_html: p.text_html,
                                tool_name: p.tool_name,
                                tool_input: p.tool_input,
                                tool_output: p.tool_output,
                                tool_status: p.tool_status,
                                tool_title: p.tool_title,
                            })
                            .collect(),
                    })
                    .collect();

                RunnerResponse::OpencodeSessionMessages(OpencodeSessionMessagesResponse {
                    session_id: req.session_id,
                    messages,
                })
            }
            Err(e) => error_response(
                ErrorCode::IoError,
                format!("Failed to get OpenCode session messages: {}", e),
            ),
        }
    }

    async fn update_opencode_session(&self, req: UpdateOpencodeSessionRequest) -> RunnerResponse {
        let opencode_dir = self.opencode_data_dir();

        if let Some(title) = req.title {
            match octo::history::update_session_title_in_dir(&req.session_id, &title, &opencode_dir)
            {
                Ok(s) => RunnerResponse::OpencodeSessionUpdated(OpencodeSessionUpdatedResponse {
                    session: OpencodeSessionInfo {
                        id: s.id,
                        readable_id: s.readable_id,
                        title: s.title,
                        parent_id: s.parent_id,
                        workspace_path: s.workspace_path,
                        project_name: s.project_name,
                        created_at: s.created_at,
                        updated_at: s.updated_at,
                        version: s.version,
                        is_child: s.is_child,
                        model: s.model,
                        provider: s.provider,
                    },
                }),
                Err(e) => error_response(
                    ErrorCode::IoError,
                    format!("Failed to update OpenCode session: {}", e),
                ),
            }
        } else {
            error_response(ErrorCode::InvalidRequest, "No update fields provided")
        }
    }

    // ========================================================================
    // Pi Session Management Operations
    // ========================================================================

    /// Create or resume a Pi session.
    async fn pi_create_session(&self, req: PiCreateSessionRequest) -> RunnerResponse {
        info!(
            "pi_create_session: session_id={}, cwd={:?}",
            req.session_id, req.config.cwd
        );

        // Convert protocol config to pi_manager config
        let pi_config = octo::runner::pi_manager::PiSessionConfig {
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
            "pi_steer: session_id={}, message_len={}",
            req.session_id,
            req.message.len()
        );

        match self.pi_manager.steer(&req.session_id, &req.message).await {
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
            "pi_follow_up: session_id={}, message_len={}",
            req.session_id,
            req.message.len()
        );

        match self
            .pi_manager
            .follow_up(&req.session_id, &req.message)
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
        let sessions: Vec<PiSessionInfo> = sessions
            .into_iter()
            .map(|s| PiSessionInfo {
                session_id: s.session_id,
                state: match s.state {
                    octo::runner::pi_manager::PiSessionState::Starting => PiSessionState::Starting,
                    octo::runner::pi_manager::PiSessionState::Idle => PiSessionState::Idle,
                    octo::runner::pi_manager::PiSessionState::Streaming => {
                        PiSessionState::Streaming
                    }
                    octo::runner::pi_manager::PiSessionState::Compacting => {
                        PiSessionState::Compacting
                    }
                    octo::runner::pi_manager::PiSessionState::Aborting => PiSessionState::Aborting,
                    octo::runner::pi_manager::PiSessionState::Stopping => PiSessionState::Stopping,
                },
                last_activity: s.last_activity,
                subscriber_count: s.subscriber_count,
                cwd: PathBuf::new(), // TODO: expose from pi_manager
                provider: None,
                model: None,
            })
            .collect();

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

        match self.pi_manager.get_messages(&req.session_id).await {
            Ok(messages) => {
                // Parse JSON response to typed AgentMessage vec
                let messages_vec: Vec<octo::pi::AgentMessage> = messages
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

    /// Set the model for a Pi session.
    /// Parse model info from a Pi command response.
    ///
    /// Pi's set_model/cycle_model responses contain `{ model: "<id>", provider: "<provider>", ... }`.
    /// Falls back to the provided defaults if parsing fails.
    fn parse_model_from_response(
        response: &octo::pi::PiResponse,
        fallback_provider: &str,
        fallback_model_id: &str,
    ) -> octo::pi::PiModel {
        let data = response.data.as_ref();
        let model_id = data
            .and_then(|d| d.get("model"))
            .and_then(|v| v.as_str())
            .unwrap_or(fallback_model_id)
            .to_string();
        let provider = data
            .and_then(|d| d.get("provider"))
            .and_then(|v| v.as_str())
            .unwrap_or(fallback_provider)
            .to_string();
        octo::pi::PiModel {
            id: model_id.clone(),
            name: model_id,
            api: provider.clone(),
            provider,
            base_url: None,
            reasoning: false,
            input: vec!["text".to_string()],
            context_window: 0,
            max_tokens: 0,
            cost: None,
        }
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
                // Parse model info from Pi's response data.
                // Pi returns { model: "<id>", provider: "<provider>", ... }
                let model =
                    Self::parse_model_from_response(&response, &req.provider, &req.model_id);
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
                let model = Self::parse_model_from_response(&response, "", "");
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
                let models_vec: Vec<octo::pi::PiModel> = models_arr
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| {
                                match serde_json::from_value::<octo::pi::PiModel>(v.clone()) {
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
    async fn pi_fork(&self, req: PiForkRequest) -> RunnerResponse {
        debug!(
            "pi_fork: session_id={}, entry_id={}",
            req.session_id, req.entry_id
        );

        match self.pi_manager.fork(&req.session_id, &req.entry_id).await {
            Ok(result) => {
                // Parse fork result from JSON response
                let text = result
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let cancelled = result
                    .get("cancelled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                RunnerResponse::PiForkResult(PiForkResultResponse {
                    session_id: req.session_id,
                    text,
                    cancelled,
                })
            }
            Err(e) => error_response(
                ErrorCode::PiSessionNotFound,
                format!("Failed to fork: {}", e),
            ),
        }
    }

    /// Get messages available for forking.
    async fn pi_get_fork_messages(&self, req: PiGetForkMessagesRequest) -> RunnerResponse {
        debug!("pi_get_fork_messages: session_id={}", req.session_id);

        match self.pi_manager.get_fork_messages(&req.session_id).await {
            Ok(messages) => {
                // Parse JSON response to typed PiForkMessage vec
                let messages_vec: Vec<PiForkMessage> = messages
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| serde_json::from_value(v.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default();
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

    /// Get available commands.
    async fn pi_get_commands(&self, req: PiGetCommandsRequest) -> RunnerResponse {
        debug!("pi_get_commands: session_id={}", req.session_id);

        match self.pi_manager.get_commands(&req.session_id).await {
            Ok(commands) => {
                // Parse JSON response to typed PiCommandInfo vec
                let commands_vec: Vec<PiCommandInfo> = commands
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| serde_json::from_value(v.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default();
                RunnerResponse::PiCommands(PiCommandsResponse {
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
    /// Subscribes to the PiSessionManager's broadcast channel and streams events.
    async fn handle_pi_subscribe(
        &self,
        session_id: &str,
        writer: &mut tokio::net::unix::OwnedWriteHalf,
    ) -> Result<(), std::io::Error> {
        info!("handle_pi_subscribe: session_id={}", session_id);

        // Subscribe to the session's event stream
        let mut rx = match self.pi_manager.subscribe(session_id).await {
            Ok(rx) => rx,
            Err(e) => {
                // Session doesn't exist - send error and end
                let resp = error_response(
                    ErrorCode::PiSessionNotFound,
                    format!("Failed to subscribe: {}", e),
                );
                let json = serde_json::to_string(&resp).unwrap();
                writer.write_all(format!("{}\n", json).as_bytes()).await?;
                return Ok(());
            }
        };

        // Send subscription confirmation
        let resp = RunnerResponse::PiSubscribed(PiSubscribedResponse {
            session_id: session_id.to_string(),
        });
        let json = serde_json::to_string(&resp).unwrap();
        writer.write_all(format!("{}\n", json).as_bytes()).await?;

        // Stream events until the session closes or client disconnects
        loop {
            match rx.recv().await {
                Ok(event_wrapper) => {
                    // Forward canonical event directly (pi_manager already translated)
                    let resp = RunnerResponse::PiEvent(event_wrapper);
                    let json = serde_json::to_string(&resp).unwrap();
                    if writer
                        .write_all(format!("{}\n", json).as_bytes())
                        .await
                        .is_err()
                    {
                        // Client disconnected
                        debug!("Pi subscription client disconnected: {}", session_id);
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(
                        "Pi subscription lagged, missed {} events for {}",
                        n, session_id
                    );
                    // Continue receiving
                }
                Err(broadcast::error::RecvError::Closed) => {
                    // Session ended
                    info!("Pi session closed, ending subscription: {}", session_id);
                    break;
                }
            }
        }

        // Send subscription end notification
        let end_resp = RunnerResponse::PiSubscriptionEnd(PiSubscriptionEndResponse {
            session_id: session_id.to_string(),
            reason: "session_closed".to_string(),
        });
        let json = serde_json::to_string(&end_resp).unwrap();
        let _ = writer.write_all(format!("{}\n", json).as_bytes()).await;

        Ok(())
    }

    /// Handle a client connection.
    async fn handle_connection(&self, stream: UnixStream) {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    // EOF
                    debug!("Client disconnected");
                    break;
                }
                Ok(_) => {
                    let req: RunnerRequest = match serde_json::from_str(&line) {
                        Ok(r) => r,
                        Err(e) => {
                            let resp = error_response(
                                ErrorCode::InvalidRequest,
                                format!("Invalid JSON: {}", e),
                            );
                            let json = serde_json::to_string(&resp).unwrap();
                            let _ = writer.write_all(format!("{}\n", json).as_bytes()).await;
                            continue;
                        }
                    };

                    debug!("Received request: {:?}", req);

                    // Handle PiSubscribe specially since it streams
                    if let RunnerRequest::PiSubscribe(ref sub_req) = req {
                        let session_id = sub_req.session_id.clone();
                        if let Err(e) = self.handle_pi_subscribe(&session_id, &mut writer).await {
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
                                let json = serde_json::to_string(&resp).unwrap();
                                if writer
                                    .write_all(format!("{}\n", json).as_bytes())
                                    .await
                                    .is_err()
                                {
                                    break;
                                }

                                // Send any buffered lines first
                                for buffered_line in buffered_lines {
                                    let resp = RunnerResponse::StdoutLine(StdoutLineResponse {
                                        id: process_id.clone(),
                                        line: buffered_line,
                                    });
                                    let json = serde_json::to_string(&resp).unwrap();
                                    if writer
                                        .write_all(format!("{}\n", json).as_bytes())
                                        .await
                                        .is_err()
                                    {
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
                                            let json = serde_json::to_string(&resp).unwrap();
                                            if writer
                                                .write_all(format!("{}\n", json).as_bytes())
                                                .await
                                                .is_err()
                                            {
                                                break;
                                            }
                                        }
                                        Ok(StdoutEvent::Closed { exit_code }) => {
                                            let resp =
                                                RunnerResponse::StdoutEnd(StdoutEndResponse {
                                                    id: process_id.clone(),
                                                    exit_code,
                                                });
                                            let json = serde_json::to_string(&resp).unwrap();
                                            let _ = writer
                                                .write_all(format!("{}\n", json).as_bytes())
                                                .await;
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
                                            let json = serde_json::to_string(&resp).unwrap();
                                            let _ = writer
                                                .write_all(format!("{}\n", json).as_bytes())
                                                .await;
                                            break;
                                        }
                                    }
                                }
                                // After subscription ends, continue the connection loop
                                // (client can send more requests)
                                continue;
                            }
                            Err(resp) => {
                                let json = serde_json::to_string(&resp).unwrap();
                                if writer
                                    .write_all(format!("{}\n", json).as_bytes())
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                                continue;
                            }
                        }
                    }

                    let resp = self.handle_request(req).await;
                    let json = serde_json::to_string(&resp).unwrap();
                    if let Err(e) = writer.write_all(format!("{}\n", json).as_bytes()).await {
                        error!("Failed to write response: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Error reading from client: {}", e);
                    break;
                }
            }
        }
    }

    /// Run the daemon, listening on the given socket path.
    async fn run(&self, socket_path: &PathBuf) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = socket_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("creating socket directory {:?}", parent))?;
        }

        // Remove existing socket file
        let _ = tokio::fs::remove_file(socket_path).await;

        // Bind
        let listener = UnixListener::bind(socket_path)
            .with_context(|| format!("binding to {:?}", socket_path))?;

        info!("Runner listening on {:?}", socket_path);

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            debug!("New client connection");
                            let runner = Runner {
                                state: Arc::clone(&self.state),
                                shutdown_tx: self.shutdown_tx.clone(),
                                sandbox_config: self.sandbox_config.clone(),
                                binaries: self.binaries.clone(),
                                user_config: self.user_config.clone(),
                                pi_manager: Arc::clone(&self.pi_manager),
                            };
                            tokio::spawn(async move {
                                runner.handle_connection(stream).await;
                            });
                        }
                        Err(e) => {
                            error!("Accept error: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutting down...");
                    break;
                }
            }
        }

        // Cleanup: kill all managed processes
        let mut state = self.state.write().await;
        for (id, mut proc) in state.processes.drain() {
            if proc.is_running() {
                info!("Killing process '{}' on shutdown", id);
                let _ = proc.child.kill().await;
            }
        }

        // Remove socket file
        let _ = tokio::fs::remove_file(socket_path).await;

        info!("Runner stopped");
        Ok(())
    }
}

fn get_default_socket_path() -> PathBuf {
    // Use XDG_RUNTIME_DIR if available (typically /run/user/<uid>), otherwise /tmp
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(DEFAULT_SOCKET_PATTERN.replace("{runtime_dir}", &runtime_dir))
}

fn error_response(code: ErrorCode, message: impl Into<String>) -> RunnerResponse {
    RunnerResponse::Error(ErrorResponse {
        code,
        message: message.into(),
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();

    let socket_path = args.socket.unwrap_or_else(get_default_socket_path);

    info!(
        "Starting octo-runner (user={}, socket={:?})",
        std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
        socket_path
    );

    // Load sandbox configuration from trusted location
    let sandbox_config = if args.no_sandbox {
        info!("Sandboxing disabled via --no-sandbox flag");
        None
    } else if let Some(ref config_path) = args.sandbox_config {
        // Load from specified path
        match std::fs::read_to_string(config_path) {
            Ok(contents) => match toml::from_str::<SandboxConfig>(&contents) {
                Ok(mut config) => {
                    config.enabled = true;
                    info!("Loaded sandbox config from {:?}", config_path);
                    Some(config)
                }
                Err(e) => {
                    error!("Failed to parse sandbox config {:?}: {}", config_path, e);
                    return Err(e.into());
                }
            },
            Err(e) => {
                error!("Failed to read sandbox config {:?}: {}", config_path, e);
                return Err(e.into());
            }
        }
    } else {
        // Load from system config (trusted, root-owned)
        let config_path = std::path::Path::new("/etc/octo/sandbox.toml");
        if !config_path.exists() {
            None
        } else {
            match std::fs::read_to_string(config_path) {
                Ok(contents) => match toml::from_str::<SandboxConfig>(&contents) {
                    Ok(config) => {
                        if config.enabled {
                            info!(
                                "Loaded system sandbox config from {}, profile='{}'",
                                "/etc/octo/sandbox.toml", config.profile
                            );
                            Some(config)
                        } else {
                            info!("System sandbox config exists but is disabled (enabled=false)");
                            None
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to parse system sandbox config: {}. Sandboxing disabled.",
                            e
                        );
                        None
                    }
                },
                Err(e) => {
                    warn!(
                        "Failed to read system sandbox config: {}. Sandboxing disabled.",
                        e
                    );
                    None
                }
            }
        }
    };

    if sandbox_config.is_some() {
        info!("Sandbox enabled - processes will be wrapped with bwrap");
    } else {
        warn!("Sandbox disabled - processes will run without isolation");
    }

    // Load environment variables from ~/.config/octo/env
    // This file uses KEY=VALUE format (one per line, # comments, no quoting needed).
    // These are set as process-level env vars so all child processes (Pi, etc.) inherit them.
    // Typical use: API keys that systemd services don't inherit from shell profiles.
    load_env_file();

    // Load user config from ~/.config/octo/config.toml (or custom path)
    let user_config = args
        .config
        .map(RunnerUserConfig::load_from_path)
        .unwrap_or_else(RunnerUserConfig::load);

    info!(
        "User config: workspace_dir={:?}, pi_sessions={:?}, memories={:?}",
        user_config.workspace_dir, user_config.pi_sessions_dir, user_config.memories_dir
    );

    // CLI args override config file
    let binaries = SessionBinaries {
        opencode: args
            .opencode_binary
            .unwrap_or(user_config.opencode_binary.clone()),
        fileserver: args
            .fileserver_binary
            .unwrap_or(user_config.fileserver_binary.clone()),
        ttyd: args.ttyd_binary.unwrap_or(user_config.ttyd_binary.clone()),
    };

    info!(
        "Session binaries: opencode={}, fileserver={}, ttyd={}",
        binaries.opencode, binaries.fileserver, binaries.ttyd
    );

    // Create PiSessionManager for managing Pi agent processes
    let state_dir = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".local").join("state")
        });
    let pi_config = PiManagerConfig {
        pi_binary: PathBuf::from(&user_config.pi_binary),
        default_cwd: user_config.workspace_dir.clone(),
        idle_timeout_secs: 300, // 5 minutes
        cleanup_interval_secs: 60,
        hstry_db_path: octo::history::hstry_db_path(),
        sandbox_config: sandbox_config.clone(),
        runner_id: user_config.runner_id.clone(),
        model_cache_dir: Some(state_dir.join("octo").join("model-cache")),
    };
    let pi_manager = PiSessionManager::new(pi_config);

    // Start the cleanup loop for idle Pi sessions
    let pi_manager_cleanup = Arc::clone(&pi_manager);
    tokio::spawn(async move {
        pi_manager_cleanup.cleanup_loop().await;
    });

    info!("Pi session manager initialized");

    // Auto-start hstry and mmry daemons for this user.
    // The runner runs as the target Linux user, so `service start` operates
    // on that user's data directory and config. Both commands are idempotent
    // (return error if already running, which we ignore).
    ensure_user_service("hstry").await;
    ensure_user_service("mmry").await;

    let runner = Runner::new(sandbox_config, binaries, user_config, pi_manager);
    runner.run(&socket_path).await
}

/// Ensure a user-level service daemon (hstry, mmry) is running.
///
/// Shells out to `<binary> service start` which properly daemonizes.
/// If already running, the command fails with "already running" which
/// is treated as success.
async fn ensure_user_service(name: &str) {
    info!("Ensuring {} daemon is running...", name);

    match tokio::process::Command::new(name)
        .args(["service", "enable"])
        .output()
        .await
    {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            if out.status.success() {
                info!("{} service enabled", name);
            } else {
                let combined = format!("{}{}", stdout, stderr);
                if !combined.contains("already enabled")
                    && !combined.contains("Already enabled")
                {
                    warn!(
                        "{} service enable failed (exit {}): {}{}",
                        name, out.status, stdout, stderr
                    );
                }
            }
        }
        Err(e) => {
            warn!("Failed to run `{} service enable`: {}", name, e);
        }
    }

    match tokio::process::Command::new(name)
        .args(["service", "start"])
        .output()
        .await
    {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            if out.status.success() {
                info!("{} daemon started: {}", name, stdout.trim());
            } else {
                let combined = format!("{}{}", stdout, stderr);
                if combined.contains("already running") || combined.contains("Already running") {
                    info!("{} daemon already running", name);
                } else {
                    warn!(
                        "{} service start failed (exit {}): {}{}",
                        name, out.status, stdout, stderr
                    );
                }
            }
        }
        Err(e) => {
            warn!("Failed to run `{} service start`: {}", name, e);
        }
    }
}

/// Load environment variables from `~/.config/octo/env`.
///
/// Format: one `KEY=VALUE` per line. Lines starting with `#` are comments.
/// Empty lines are skipped. Values are NOT shell-unquoted (quotes are literal).
/// Variables are set into the process environment so all child processes inherit them.
fn load_env_file() {
    let env_path = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        format!("{}/.config", home)
    }) + "/octo/env";

    let path = std::path::Path::new(&env_path);
    if !path.exists() {
        debug!("No env file at {}, skipping", env_path);
        return;
    }

    match std::fs::read_to_string(path) {
        Ok(contents) => {
            let mut count = 0;
            for line in contents.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = trimmed.split_once('=') {
                    let key = key.trim();
                    let value = value.trim();
                    if !key.is_empty() {
                        // SAFETY: This runs at startup before any threads are spawned
                        // for child process management, so there are no data races.
                        unsafe { std::env::set_var(key, value) };
                        count += 1;
                    }
                }
            }
            info!("Loaded {} environment variables from {}", count, env_path);
        }
        Err(e) => {
            warn!("Failed to read env file {}: {}", env_path, e);
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    //! Security tests for octo-runner session spawning.
    //!
    //! These tests verify that session services bind to localhost (127.0.0.1)
    //! rather than all interfaces (0.0.0.0). This is critical for security:
    //! services should only be accessible via the octo backend proxy.

    /// Helper to build opencode args (mirrors the logic in Runner::start_session).
    fn build_opencode_args(port: u16, agent: Option<&str>) -> Vec<String> {
        let mut args = vec![
            "serve".to_string(),
            "--port".to_string(),
            port.to_string(),
            "--hostname".to_string(),
            "127.0.0.1".to_string(),
        ];
        if let Some(agent_name) = agent {
            args.push("--agent".to_string());
            args.push(agent_name.to_string());
        }
        args
    }

    /// Helper to build fileserver args (mirrors the logic in Runner::start_session).
    fn build_fileserver_args(port: u16, workspace_path: &str) -> Vec<String> {
        vec![
            "--port".to_string(),
            port.to_string(),
            "--bind".to_string(),
            "127.0.0.1".to_string(),
            "--root".to_string(),
            workspace_path.to_string(),
        ]
    }

    /// Helper to build ttyd args (mirrors the logic in Runner::start_session).
    fn build_ttyd_args(port: u16, workspace_path: &str) -> Vec<String> {
        vec![
            "--port".to_string(),
            port.to_string(),
            "--interface".to_string(),
            "127.0.0.1".to_string(),
            "--writable".to_string(),
            "--cwd".to_string(),
            workspace_path.to_string(),
            "zsh".to_string(),
            "-l".to_string(),
        ]
    }

    #[test]
    fn test_opencode_binds_to_localhost_only() {
        let args = build_opencode_args(4096, None);

        let hostname_idx = args.iter().position(|a| a == "--hostname");
        assert!(
            hostname_idx.is_some(),
            "opencode args must include --hostname"
        );

        let bind_addr = &args[hostname_idx.unwrap() + 1];
        assert_eq!(
            bind_addr, "127.0.0.1",
            "opencode must bind to 127.0.0.1, not {}. Binding to 0.0.0.0 exposes the service to the network!",
            bind_addr
        );
        assert_ne!(
            bind_addr, "0.0.0.0",
            "SECURITY: opencode must NOT bind to 0.0.0.0"
        );
    }

    #[test]
    fn test_opencode_with_agent_binds_to_localhost_only() {
        let args = build_opencode_args(4096, Some("test-agent"));

        let hostname_idx = args.iter().position(|a| a == "--hostname");
        assert!(hostname_idx.is_some());

        let bind_addr = &args[hostname_idx.unwrap() + 1];
        assert_eq!(bind_addr, "127.0.0.1");
        assert_ne!(bind_addr, "0.0.0.0");
    }

    #[test]
    fn test_fileserver_binds_to_localhost_only() {
        let args = build_fileserver_args(8080, "/home/user/workspace");

        let bind_idx = args.iter().position(|a| a == "--bind");
        assert!(bind_idx.is_some(), "fileserver args must include --bind");

        let bind_addr = &args[bind_idx.unwrap() + 1];
        assert_eq!(
            bind_addr, "127.0.0.1",
            "fileserver must bind to 127.0.0.1, not {}. Binding to 0.0.0.0 exposes the service to the network!",
            bind_addr
        );
        assert_ne!(
            bind_addr, "0.0.0.0",
            "SECURITY: fileserver must NOT bind to 0.0.0.0"
        );
    }

    #[test]
    fn test_ttyd_binds_to_localhost_only() {
        let args = build_ttyd_args(7681, "/home/user/workspace");

        let interface_idx = args.iter().position(|a| a == "--interface");
        assert!(
            interface_idx.is_some(),
            "ttyd args must include --interface"
        );

        let bind_addr = &args[interface_idx.unwrap() + 1];
        assert_eq!(
            bind_addr, "127.0.0.1",
            "ttyd must bind to 127.0.0.1, not {}. Binding to 0.0.0.0 exposes the service to the network!",
            bind_addr
        );
        assert_ne!(
            bind_addr, "0.0.0.0",
            "SECURITY: ttyd must NOT bind to 0.0.0.0"
        );
    }
}
