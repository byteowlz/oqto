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
use tokio::sync::{Mutex, RwLock, broadcast};

use octo::runner::client::DEFAULT_SOCKET_PATTERN;
use octo::runner::protocol::*;

#[derive(Parser, Debug)]
#[command(
    name = "octo-runner",
    about = "Process runner daemon for multi-user isolation"
)]
struct Args {
    /// Socket path to listen on.
    /// Defaults to /run/octo/runner-{username}.sock
    #[arg(short, long)]
    socket: Option<PathBuf>,

    /// Enable verbose logging.
    #[arg(short, long)]
    verbose: bool,
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
            RunnerRequest::SubscribeStdout(_) => {
                // Handled specially in handle_connection since it streams
                error_response(ErrorCode::Internal, "SubscribeStdout must be handled via streaming")
            }
        }
    }

    /// Get stdout broadcast receiver for a process.
    async fn get_stdout_receiver(&self, process_id: &str) -> Result<(broadcast::Receiver<StdoutEvent>, Vec<String>), RunnerResponse> {
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
            return Err(error_response(ErrorCode::IoError, "stdout channel not available"));
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
            Ok(mut child) => {
                let pid = child.id().unwrap_or(0);
                info!(
                    "Spawned process '{}': {} {:?} (pid={}, rpc={})",
                    req.id, req.binary, req.args, pid, is_rpc
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
                        Self::stdout_reader_task(process_id, stdout, stderr, buffer_clone, tx_clone).await;
                    });

                    (Some(buffer), Some(tx), Some(handle))
                } else {
                    (None, None, None)
                };

                let managed = ManagedProcess {
                    id: req.id.clone(),
                    pid,
                    binary: req.binary,
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
        let _ = stdout_tx.send(StdoutEvent::Closed { exit_code: buf.exit_code });
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

                    // Handle SubscribeStdout specially since it streams
                    if let RunnerRequest::SubscribeStdout(ref sub_req) = req {
                        let process_id = sub_req.id.clone();
                        match self.get_stdout_receiver(&process_id).await {
                            Ok((mut rx, buffered_lines)) => {
                                // Send subscription confirmation
                                let resp = RunnerResponse::StdoutSubscribed(StdoutSubscribedResponse {
                                    id: process_id.clone(),
                                });
                                let json = serde_json::to_string(&resp).unwrap();
                                if writer.write_all(format!("{}\n", json).as_bytes()).await.is_err() {
                                    break;
                                }

                                // Send any buffered lines first
                                for buffered_line in buffered_lines {
                                    let resp = RunnerResponse::StdoutLine(StdoutLineResponse {
                                        id: process_id.clone(),
                                        line: buffered_line,
                                    });
                                    let json = serde_json::to_string(&resp).unwrap();
                                    if writer.write_all(format!("{}\n", json).as_bytes()).await.is_err() {
                                        break;
                                    }
                                }

                                // Stream new lines as they arrive
                                loop {
                                    match rx.recv().await {
                                        Ok(StdoutEvent::Line(stdout_line)) => {
                                            let resp = RunnerResponse::StdoutLine(StdoutLineResponse {
                                                id: process_id.clone(),
                                                line: stdout_line,
                                            });
                                            let json = serde_json::to_string(&resp).unwrap();
                                            if writer.write_all(format!("{}\n", json).as_bytes()).await.is_err() {
                                                break;
                                            }
                                        }
                                        Ok(StdoutEvent::Closed { exit_code }) => {
                                            let resp = RunnerResponse::StdoutEnd(StdoutEndResponse {
                                                id: process_id.clone(),
                                                exit_code,
                                            });
                                            let json = serde_json::to_string(&resp).unwrap();
                                            let _ = writer.write_all(format!("{}\n", json).as_bytes()).await;
                                            break;
                                        }
                                        Err(broadcast::error::RecvError::Lagged(n)) => {
                                            warn!("Stdout subscription lagged, missed {} events", n);
                                            // Continue receiving
                                        }
                                        Err(broadcast::error::RecvError::Closed) => {
                                            let resp = RunnerResponse::StdoutEnd(StdoutEndResponse {
                                                id: process_id.clone(),
                                                exit_code: None,
                                            });
                                            let json = serde_json::to_string(&resp).unwrap();
                                            let _ = writer.write_all(format!("{}\n", json).as_bytes()).await;
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
                                if writer.write_all(format!("{}\n", json).as_bytes()).await.is_err() {
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
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| "/tmp".to_string());
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

    let runner = Runner::new();
    runner.run(&socket_path).await
}
