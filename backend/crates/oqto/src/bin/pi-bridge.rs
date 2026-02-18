//! pi-bridge - HTTP/WebSocket bridge for Pi RPC mode.
//!
//! This service runs inside containers and provides HTTP/WebSocket access
//! to a Pi process running in RPC mode. It bridges between HTTP requests
//! and Pi's stdin/stdout JSON-RPC protocol.
//!
//! ## Endpoints
//!
//! - `POST /command` - Send a command to Pi, returns JSON response
//! - `GET /ws` - WebSocket for bidirectional command/event streaming
//! - `GET /health` - Health check
//!
//! ## Usage
//!
//! ```bash
//! # Start with default settings
//! pi-bridge
//!
//! # Custom port and Pi executable
//! pi-bridge --port 41824 --pi-executable /usr/local/bin/pi
//! ```

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use clap::Parser;
use futures::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock, broadcast, mpsc};

#[derive(Parser, Debug)]
#[command(name = "pi-bridge", about = "HTTP/WebSocket bridge for Pi RPC mode")]
struct Args {
    /// Port to listen on.
    #[arg(short, long, default_value = "41824")]
    port: u16,

    /// Host to bind to.
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    /// Path to the Pi executable.
    #[arg(long, default_value = "pi")]
    pi_executable: String,

    /// Working directory for Pi.
    #[arg(long, default_value = ".")]
    work_dir: PathBuf,

    /// Continue previous session.
    #[arg(long)]
    continue_session: bool,

    /// Provider to use.
    #[arg(long)]
    provider: Option<String>,

    /// Model to use.
    #[arg(long)]
    model: Option<String>,

    /// Enable verbose logging.
    #[arg(short, long)]
    verbose: bool,
}

/// Shared state for the bridge.
struct BridgeState {
    /// Channel to send commands to Pi.
    command_tx: mpsc::Sender<String>,
    /// Broadcast channel for events from Pi.
    event_tx: broadcast::Sender<PiMessage>,
    /// Pending response receivers (keyed by request ID).
    pending_responses: RwLock<HashMap<String, tokio::sync::oneshot::Sender<Value>>>,
    /// Counter for generating unique request IDs.
    request_counter: Mutex<u64>,
    /// Flag indicating if Pi is running.
    running: RwLock<bool>,
}

/// Message from Pi (either response or event).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum PiMessage {
    Response(PiResponse),
    Event(Value),
}

/// Response from Pi.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PiResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Command request from HTTP client.
#[derive(Debug, Deserialize)]
struct CommandRequest {
    #[serde(flatten)]
    command: Value,
}

/// Command response to HTTP client.
#[derive(Debug, Serialize)]
struct CommandResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Health check response.
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    pi_running: bool,
}

impl BridgeState {
    async fn next_request_id(&self) -> String {
        let mut counter = self.request_counter.lock().await;
        *counter += 1;
        format!("bridge-req-{}", *counter)
    }
}

/// Spawn Pi process and return the child along with stdin/stdout handles.
async fn spawn_pi(
    args: &Args,
) -> Result<(
    Child,
    tokio::process::ChildStdin,
    tokio::process::ChildStdout,
)> {
    let mut cmd = Command::new(&args.pi_executable);
    cmd.arg("--mode").arg("rpc");

    if args.continue_session {
        cmd.arg("--continue");
    }

    cmd.current_dir(&args.work_dir);

    if let Some(ref provider) = args.provider {
        cmd.arg("--provider").arg(provider);
    }

    if let Some(ref model) = args.model {
        cmd.arg("--model").arg(model);
    }

    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().context("failed to spawn Pi process")?;

    let stdin = child.stdin.take().context("Pi has no stdin")?;
    let stdout = child.stdout.take().context("Pi has no stdout")?;

    // Stderr reader task
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    warn!("Pi stderr: {}", line);
                }
            }
        });
    }

    Ok((child, stdin, stdout))
}

/// Start the stdin writer task.
fn start_stdin_writer(stdin: tokio::process::ChildStdin, mut command_rx: mpsc::Receiver<String>) {
    tokio::spawn(async move {
        let mut stdin = stdin;
        while let Some(command) = command_rx.recv().await {
            let line = format!("{}\n", command);
            if let Err(e) = stdin.write_all(line.as_bytes()).await {
                error!("Failed to write to Pi stdin: {:?}", e);
                break;
            }
            if let Err(e) = stdin.flush().await {
                error!("Failed to flush Pi stdin: {:?}", e);
                break;
            }
        }
        info!("Pi stdin writer task ended");
    });
}

/// Start the stdout reader task.
fn start_stdout_reader(stdout: tokio::process::ChildStdout, state: Arc<BridgeState>) {
    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            let display_line: String = line.chars().take(200).collect();
            debug!("Pi output: {}", display_line);

            // Try to parse as JSON
            match serde_json::from_str::<Value>(&line) {
                Ok(value) => {
                    // Check if it's a response (has "success" field)
                    if value.get("success").is_some() {
                        if let Some(id) = value.get("id").and_then(|v| v.as_str()) {
                            let mut pending = state.pending_responses.write().await;
                            if let Some(tx) = pending.remove(id) {
                                let _ = tx.send(value.clone());
                            }
                        }
                        // Also broadcast responses
                        let _ = state.event_tx.send(PiMessage::Response(
                            serde_json::from_value(value).unwrap_or(PiResponse {
                                success: false,
                                id: None,
                                data: None,
                                error: Some("parse error".to_string()),
                            }),
                        ));
                    } else {
                        // It's an event
                        let _ = state.event_tx.send(PiMessage::Event(value));
                    }
                }
                Err(e) => {
                    warn!("Failed to parse Pi output: {:?}, line: {}", e, display_line);
                }
            }
        }

        *state.running.write().await = false;
        info!("Pi stdout reader task ended");
    });
}

/// POST /command - Send a command to Pi.
async fn handle_command(
    State(state): State<Arc<BridgeState>>,
    Json(request): Json<CommandRequest>,
) -> impl IntoResponse {
    if !*state.running.read().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(CommandResponse {
                success: false,
                data: None,
                error: Some("Pi process not running".to_string()),
            }),
        );
    }

    // Add request ID to command
    let request_id = state.next_request_id().await;
    let mut command = request.command;
    if let Some(obj) = command.as_object_mut() {
        obj.insert("id".to_string(), Value::String(request_id.clone()));
    }

    // Set up response channel
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
    {
        let mut pending = state.pending_responses.write().await;
        pending.insert(request_id.clone(), response_tx);
    }

    // Send command
    let json = match serde_json::to_string(&command) {
        Ok(j) => j,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(CommandResponse {
                    success: false,
                    data: None,
                    error: Some(format!("Failed to serialize command: {}", e)),
                }),
            );
        }
    };

    if let Err(e) = state.command_tx.send(json).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(CommandResponse {
                success: false,
                data: None,
                error: Some(format!("Failed to send command: {}", e)),
            }),
        );
    }

    // Wait for response with timeout
    match tokio::time::timeout(std::time::Duration::from_secs(30), response_rx).await {
        Ok(Ok(response)) => {
            let success = response
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let data = response.get("data").cloned();
            let error = response
                .get("error")
                .and_then(|v| v.as_str())
                .map(String::from);

            (
                StatusCode::OK,
                Json(CommandResponse {
                    success,
                    data,
                    error,
                }),
            )
        }
        Ok(Err(_)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(CommandResponse {
                success: false,
                data: None,
                error: Some("Response channel closed".to_string()),
            }),
        ),
        Err(_) => {
            // Remove from pending
            let mut pending = state.pending_responses.write().await;
            pending.remove(&request_id);

            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(CommandResponse {
                    success: false,
                    data: None,
                    error: Some("Timeout waiting for Pi response".to_string()),
                }),
            )
        }
    }
}

/// GET /ws - WebSocket for bidirectional communication.
async fn handle_ws(
    State(state): State<Arc<BridgeState>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_connection(state, socket))
}

async fn handle_ws_connection(state: Arc<BridgeState>, socket: WebSocket) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let mut event_rx = state.event_tx.subscribe();

    // Task to forward events to WebSocket
    let state_clone = Arc::clone(&state);
    let forward_task = tokio::spawn(async move {
        while let Ok(msg) = event_rx.recv().await {
            let json = match serde_json::to_string(&msg) {
                Ok(j) => j,
                Err(e) => {
                    error!("Failed to serialize event: {:?}", e);
                    continue;
                }
            };
            if ws_tx.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Task to forward WebSocket commands to Pi
    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            Message::Text(text) => {
                // Parse as JSON and add ID if not present
                match serde_json::from_str::<Value>(&text) {
                    Ok(mut command) => {
                        if command.get("id").is_none() {
                            let request_id = state_clone.next_request_id().await;
                            if let Some(obj) = command.as_object_mut() {
                                obj.insert("id".to_string(), Value::String(request_id));
                            }
                        }
                        let json = serde_json::to_string(&command).unwrap_or(text.to_string());
                        if let Err(e) = state_clone.command_tx.send(json).await {
                            error!("Failed to send command from WS: {:?}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("Invalid JSON from WebSocket: {:?}", e);
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    forward_task.abort();
    info!("WebSocket connection closed");
}

/// GET /health - Health check.
async fn handle_health(State(state): State<Arc<BridgeState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        pi_running: *state.running.read().await,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();

    info!(
        "Starting pi-bridge on {}:{}, work_dir={:?}",
        args.host, args.port, args.work_dir
    );

    // Spawn Pi process
    let (mut child, stdin, stdout) = spawn_pi(&args).await?;
    info!("Pi process spawned with PID {:?}", child.id());

    // Create channels
    let (command_tx, command_rx) = mpsc::channel::<String>(64);
    let (event_tx, _) = broadcast::channel::<PiMessage>(256);

    // Create shared state
    let state = Arc::new(BridgeState {
        command_tx,
        event_tx,
        pending_responses: RwLock::new(HashMap::new()),
        request_counter: Mutex::new(0),
        running: RwLock::new(true),
    });

    // Start stdin writer task
    start_stdin_writer(stdin, command_rx);

    // Start stdout reader task
    start_stdout_reader(stdout, Arc::clone(&state));

    // Build router
    let app = Router::new()
        .route("/command", post(handle_command))
        .route("/ws", get(handle_ws))
        .route("/health", get(handle_health))
        .with_state(state.clone());

    // Start server
    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    let listener = TcpListener::bind(addr).await?;
    info!("pi-bridge listening on {}", addr);

    // Monitor Pi process
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        match child.wait().await {
            Ok(status) => info!("Pi process exited with status: {:?}", status),
            Err(e) => error!("Error waiting for Pi process: {:?}", e),
        }
        *state_clone.running.write().await = false;
    });

    // Serve
    axum::serve(listener, app).await?;

    Ok(())
}
