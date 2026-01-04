//! octo-runner - Process runner daemon for multi-user isolation.
//!
//! This daemon runs as a specific Linux user (via systemd user service) and
//! accepts commands over a Unix socket to spawn and manage processes.
//!
//! ## Usage
//!
//! ```bash
//! # Run with default socket path (/run/octo/runner-{username}.sock)
//! octo-runner
//!
//! # Run with custom socket path
//! octo-runner --socket /tmp/my-runner.sock
//! ```

use anyhow::{Context, Result};
use clap::Parser;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, RwLock};

use octo::runner::*;

#[derive(Parser, Debug)]
#[command(name = "octo-runner", about = "Process runner daemon for multi-user isolation")]
struct Args {
    /// Socket path to listen on.
    /// Defaults to /run/octo/runner-{username}.sock
    #[arg(short, long)]
    socket: Option<PathBuf>,

    /// Enable verbose logging.
    #[arg(short, long)]
    verbose: bool,
}

/// Managed process with optional RPC pipes.
struct ManagedProcess {
    id: String,
    pid: u32,
    binary: String,
    cwd: PathBuf,
    child: Child,
    is_rpc: bool,
    /// Buffered stdout for RPC processes.
    stdout_buffer: Vec<String>,
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
    processes: HashMap<String, ManagedProcess>,
}

impl RunnerState {
    fn new() -> Self {
        Self {
            processes: HashMap::new(),
        }
    }
}

/// The runner daemon.
struct Runner {
    state: Arc<RwLock<RunnerState>>,
    shutdown_tx: broadcast::Sender<()>,
}

impl Runner {
    fn new() -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self {
            state: Arc::new(RwLock::new(RunnerState::new())),
            shutdown_tx,
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
        }
    }

    async fn spawn_process(&self, req: SpawnProcessRequest, is_rpc: bool) -> RunnerResponse {
        let mut state = self.state.write().await;

        // Check if ID already exists
        if state.processes.contains_key(&req.id) {
            return RunnerResponse::error(
                ErrorCode::ProcessAlreadyExists,
                format!("Process with ID '{}' already exists", req.id),
            );
        }

        // Build command
        let mut cmd = Command::new(&req.binary);
        cmd.args(&req.args);
        cmd.current_dir(&req.cwd);
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
            Ok(child) => {
                let pid = child.id().unwrap_or(0);
                info!(
                    "Spawned process '{}': {} {:?} (pid={}, rpc={})",
                    req.id, req.binary, req.args, pid, is_rpc
                );

                let managed = ManagedProcess {
                    id: req.id.clone(),
                    pid,
                    binary: req.binary,
                    cwd: req.cwd,
                    child,
                    is_rpc,
                    stdout_buffer: Vec::new(),
                };

                state.processes.insert(req.id.clone(), managed);

                RunnerResponse::ProcessSpawned(ProcessSpawnedResponse { id: req.id, pid })
            }
            Err(e) => {
                error!("Failed to spawn process '{}': {}", req.id, e);
                RunnerResponse::error(ErrorCode::SpawnFailed, e.to_string())
            }
        }
    }

    async fn kill_process(&self, req: KillProcessRequest) -> RunnerResponse {
        let mut state = self.state.write().await;

        let Some(proc) = state.processes.get_mut(&req.id) else {
            return RunnerResponse::error(
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

        // Remove from tracking
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
            return RunnerResponse::error(
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
            return RunnerResponse::error(
                ErrorCode::ProcessNotFound,
                format!("Process '{}' not found", req.id),
            );
        };

        if !proc.is_rpc {
            return RunnerResponse::error(
                ErrorCode::NotRpcProcess,
                format!("Process '{}' is not an RPC process", req.id),
            );
        }

        let Some(stdin) = proc.child.stdin.as_mut() else {
            return RunnerResponse::error(ErrorCode::IoError, "stdin not available");
        };

        match stdin.write_all(req.data.as_bytes()).await {
            Ok(()) => {
                let bytes_written = req.data.len();
                debug!("Wrote {} bytes to stdin of '{}'", bytes_written, req.id);
                RunnerResponse::StdinWritten(StdinWrittenResponse {
                    id: req.id,
                    bytes_written,
                })
            }
            Err(e) => RunnerResponse::error(ErrorCode::IoError, e.to_string()),
        }
    }

    async fn read_stdout(&self, req: ReadStdoutRequest) -> RunnerResponse {
        let mut state = self.state.write().await;

        let Some(proc) = state.processes.get_mut(&req.id) else {
            return RunnerResponse::error(
                ErrorCode::ProcessNotFound,
                format!("Process '{}' not found", req.id),
            );
        };

        if !proc.is_rpc {
            return RunnerResponse::error(
                ErrorCode::NotRpcProcess,
                format!("Process '{}' is not an RPC process", req.id),
            );
        }

        // Return buffered data if available
        if !proc.stdout_buffer.is_empty() {
            let data = proc.stdout_buffer.join("");
            proc.stdout_buffer.clear();
            return RunnerResponse::StdoutRead(StdoutReadResponse {
                id: req.id,
                data,
                has_more: false, // We don't know without blocking
            });
        }

        // Try to read without blocking (timeout=0) or with timeout
        let Some(_stdout) = proc.child.stdout.as_mut() else {
            return RunnerResponse::error(ErrorCode::IoError, "stdout not available");
        };

        // For now, just return empty if nothing buffered
        // A proper implementation would use non-blocking reads or select
        RunnerResponse::StdoutRead(StdoutReadResponse {
            id: req.id,
            data: String::new(),
            has_more: false,
        })
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
                            let resp = RunnerResponse::error(
                                ErrorCode::InvalidRequest,
                                format!("Invalid JSON: {}", e),
                            );
                            let json = serde_json::to_string(&resp).unwrap();
                            let _ = writer.write_all(format!("{}\n", json).as_bytes()).await;
                            continue;
                        }
                    };

                    debug!("Received request: {:?}", req);
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
    let username = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
    PathBuf::from(DEFAULT_SOCKET_PATTERN.replace("{user}", &username))
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

    let runner = Runner::new();
    runner.run(&socket_path).await
}
