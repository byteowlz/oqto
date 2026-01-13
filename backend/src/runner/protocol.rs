//! Runner RPC protocol types.
//!
//! Defines the request/response types for communication between octo and the runner daemon.
//! The protocol uses JSON over Unix sockets with newline-delimited messages.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Request sent from octo to the runner.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunnerRequest {
    /// Spawn a detached process (fire and forget, no stdin/stdout).
    SpawnProcess(SpawnProcessRequest),

    /// Spawn a process with stdin/stdout pipes for RPC communication.
    /// Used for Pi agent which communicates via JSON-RPC over stdio.
    SpawnRpcProcess(SpawnRpcProcessRequest),

    /// Kill a process by PID.
    KillProcess(KillProcessRequest),

    /// Get status of a process.
    GetStatus(GetStatusRequest),

    /// List all managed processes.
    ListProcesses,

    /// Send data to a process's stdin (for RPC processes).
    WriteStdin(WriteStdinRequest),

    /// Read available data from a process's stdout (for RPC processes).
    ReadStdout(ReadStdoutRequest),

    /// Subscribe to stdout stream (for RPC processes).
    /// Lines are pushed as they arrive via StdoutLine responses.
    /// The subscription ends when the process exits or client disconnects.
    SubscribeStdout(SubscribeStdoutRequest),

    /// Health check.
    Ping,

    /// Shutdown the runner gracefully.
    Shutdown,
}

/// Response from runner to octo.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunnerResponse {
    /// Process spawned successfully.
    ProcessSpawned(ProcessSpawnedResponse),

    /// Process killed.
    ProcessKilled(ProcessKilledResponse),

    /// Process status.
    ProcessStatus(ProcessStatusResponse),

    /// List of managed processes.
    ProcessList(ProcessListResponse),

    /// Data written to stdin.
    StdinWritten(StdinWrittenResponse),

    /// Data read from stdout.
    StdoutRead(StdoutReadResponse),

    /// Subscription to stdout started.
    StdoutSubscribed(StdoutSubscribedResponse),

    /// A line from stdout (pushed during subscription).
    StdoutLine(StdoutLineResponse),

    /// Stdout subscription ended (process exited).
    StdoutEnd(StdoutEndResponse),

    /// Pong response to ping.
    Pong,

    /// Shutdown acknowledged.
    ShuttingDown,

    /// Error response.
    Error(ErrorResponse),
}

// ============================================================================
// Request types
// ============================================================================

/// Request to spawn a detached process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnProcessRequest {
    /// Unique ID for this process (provided by caller for tracking).
    pub id: String,
    /// Path to the binary to execute.
    pub binary: String,
    /// Command line arguments.
    pub args: Vec<String>,
    /// Working directory.
    pub cwd: PathBuf,
    /// Environment variables (merged with runner's environment).
    pub env: HashMap<String, String>,
}

/// Request to spawn a process with RPC pipes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnRpcProcessRequest {
    /// Unique ID for this process (provided by caller for tracking).
    pub id: String,
    /// Path to the binary to execute.
    pub binary: String,
    /// Command line arguments.
    pub args: Vec<String>,
    /// Working directory.
    pub cwd: PathBuf,
    /// Environment variables (merged with runner's environment).
    pub env: HashMap<String, String>,
}

/// Request to kill a process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillProcessRequest {
    /// Process ID assigned by the runner.
    pub id: String,
    /// Force kill (SIGKILL) instead of graceful (SIGTERM).
    #[serde(default)]
    pub force: bool,
}

/// Request to get process status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetStatusRequest {
    /// Process ID assigned by the runner.
    pub id: String,
}

/// Request to write to process stdin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteStdinRequest {
    /// Process ID.
    pub id: String,
    /// Data to write (will be UTF-8 encoded).
    pub data: String,
}

/// Request to read from process stdout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadStdoutRequest {
    /// Process ID.
    pub id: String,
    /// Timeout in milliseconds (0 = non-blocking).
    #[serde(default)]
    pub timeout_ms: u64,
}

/// Request to subscribe to stdout stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeStdoutRequest {
    /// Process ID.
    pub id: String,
}

// ============================================================================
// Response types
// ============================================================================

/// Response when a process is spawned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSpawnedResponse {
    /// The ID provided in the request.
    pub id: String,
    /// OS process ID.
    pub pid: u32,
}

/// Response when a process is killed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessKilledResponse {
    /// The process ID.
    pub id: String,
    /// Whether the process was actually running when killed.
    pub was_running: bool,
}

/// Process status response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessStatusResponse {
    /// The process ID.
    pub id: String,
    /// Whether the process is currently running.
    pub running: bool,
    /// OS process ID (if known).
    pub pid: Option<u32>,
    /// Exit code if process has exited.
    pub exit_code: Option<i32>,
}

/// List of all managed processes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessListResponse {
    /// List of process info.
    pub processes: Vec<ProcessInfo>,
}

/// Information about a managed process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    /// Process ID assigned by runner.
    pub id: String,
    /// OS process ID.
    pub pid: u32,
    /// Binary name.
    pub binary: String,
    /// Working directory.
    pub cwd: PathBuf,
    /// Whether this is an RPC process (has stdin/stdout pipes).
    pub is_rpc: bool,
    /// Whether currently running.
    pub running: bool,
}

/// Response when stdin data is written.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdinWrittenResponse {
    /// Process ID.
    pub id: String,
    /// Number of bytes written.
    pub bytes_written: usize,
}

/// Response with stdout data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdoutReadResponse {
    /// Process ID.
    pub id: String,
    /// Data read from stdout.
    pub data: String,
    /// Whether there's more data available.
    pub has_more: bool,
}

/// Response confirming stdout subscription started.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdoutSubscribedResponse {
    /// Process ID.
    pub id: String,
}

/// A line from stdout (pushed during subscription).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdoutLineResponse {
    /// Process ID.
    pub id: String,
    /// The line content.
    pub line: String,
}

/// Stdout subscription ended.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdoutEndResponse {
    /// Process ID.
    pub id: String,
    /// Exit code if process exited.
    pub exit_code: Option<i32>,
}

/// Error response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error code.
    pub code: ErrorCode,
    /// Human-readable error message.
    pub message: String,
}

/// Error codes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Process not found.
    ProcessNotFound,
    /// Process already exists with this ID.
    ProcessAlreadyExists,
    /// Failed to spawn process.
    SpawnFailed,
    /// Failed to kill process.
    KillFailed,
    /// Process is not an RPC process (no stdin/stdout).
    NotRpcProcess,
    /// IO error.
    IoError,
    /// Invalid request.
    InvalidRequest,
    /// Internal error.
    Internal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = RunnerRequest::SpawnProcess(SpawnProcessRequest {
            id: "proc-1".to_string(),
            binary: "/usr/bin/opencode".to_string(),
            args: vec![
                "serve".to_string(),
                "--port".to_string(),
                "8080".to_string(),
            ],
            cwd: PathBuf::from("/home/user/project"),
            env: HashMap::from([("FOO".to_string(), "bar".to_string())]),
        });

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("spawn_process"));
        assert!(json.contains("proc-1"));

        let parsed: RunnerRequest = serde_json::from_str(&json).unwrap();
        match parsed {
            RunnerRequest::SpawnProcess(p) => {
                assert_eq!(p.id, "proc-1");
                assert_eq!(p.binary, "/usr/bin/opencode");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_response_serialization() {
        let resp = RunnerResponse::ProcessSpawned(ProcessSpawnedResponse {
            id: "proc-1".to_string(),
            pid: 12345,
        });

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("process_spawned"));
        assert!(json.contains("12345"));
    }

    #[test]
    fn test_error_response() {
        let resp = RunnerResponse::Error(ErrorResponse {
            code: ErrorCode::ProcessNotFound,
            message: "No such process: foo".to_string(),
        });

        match resp {
            RunnerResponse::Error(e) => {
                assert_eq!(e.code, ErrorCode::ProcessNotFound);
                assert!(e.message.contains("foo"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_ping_pong() {
        let req = RunnerRequest::Ping;
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("ping"));

        let resp = RunnerResponse::Pong;
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("pong"));
    }
}
