//! Runner client for communicating with octo-runner daemon.
//!
//! Provides a high-level async API for spawning and managing processes
//! through the runner daemon via Unix socket.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use super::protocol::*;

/// Default socket path pattern. {user} is replaced with the username.
/// Uses XDG_RUNTIME_DIR if available, otherwise falls back to /tmp.
pub const DEFAULT_SOCKET_PATTERN: &str = "{runtime_dir}/octo-runner.sock";

/// Client for communicating with the runner daemon.
#[derive(Clone)]
pub struct RunnerClient {
    socket_path: PathBuf,
}

impl RunnerClient {
    /// Create a new runner client for the given socket path.
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    /// Create a runner client using the default socket path.
    /// Uses XDG_RUNTIME_DIR if available, otherwise /tmp.
    pub fn default() -> Self {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
        let socket_path = DEFAULT_SOCKET_PATTERN.replace("{runtime_dir}", &runtime_dir);
        Self::new(socket_path)
    }

    /// Create a runner client for a specific user using the default socket pattern.
    #[allow(dead_code)]
    pub fn for_user(username: &str) -> Self {
        // Legacy method - now just uses default
        let _ = username;
        Self::default()
    }

    /// Get the socket path.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Send a request and receive a response.
    async fn request(&self, req: &RunnerRequest) -> Result<RunnerResponse> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .await
            .with_context(|| format!("connecting to runner at {:?}", self.socket_path))?;

        // Send request as JSON line
        let mut json = serde_json::to_string(req).context("serializing request")?;
        json.push('\n');
        stream
            .write_all(json.as_bytes())
            .await
            .context("writing request")?;

        // Read response line
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .context("reading response")?;

        let resp: RunnerResponse = serde_json::from_str(&line).context("parsing response")?;

        // Check for error response
        if let RunnerResponse::Error(e) = &resp {
            anyhow::bail!("runner error ({:?}): {}", e.code, e.message);
        }

        Ok(resp)
    }

    /// Spawn an RPC process with stdin/stdout pipes.
    /// Spawn an RPC process (with stdin/stdout pipes).
    ///
    /// If `sandboxed` is true, the runner will wrap the process in a sandbox
    /// using its trusted configuration from `/etc/octo/sandbox.toml`.
    pub async fn spawn_rpc_process(
        &self,
        id: impl Into<String>,
        binary: impl Into<String>,
        args: Vec<String>,
        cwd: impl Into<PathBuf>,
        env: HashMap<String, String>,
        sandboxed: bool,
    ) -> Result<u32> {
        let req = RunnerRequest::SpawnRpcProcess(SpawnRpcProcessRequest {
            id: id.into(),
            binary: binary.into(),
            args,
            cwd: cwd.into(),
            env,
            sandboxed,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::ProcessSpawned(p) => Ok(p.pid),
            _ => anyhow::bail!("unexpected response to spawn_rpc_process"),
        }
    }

    /// Get process status.
    pub async fn get_status(&self, id: impl Into<String>) -> Result<ProcessStatusResponse> {
        let req = RunnerRequest::GetStatus(GetStatusRequest { id: id.into() });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::ProcessStatus(s) => Ok(s),
            _ => anyhow::bail!("unexpected response to get_status"),
        }
    }

    /// Write data to a process's stdin.
    pub async fn write_stdin(
        &self,
        id: impl Into<String>,
        data: impl Into<String>,
    ) -> Result<usize> {
        let req = RunnerRequest::WriteStdin(WriteStdinRequest {
            id: id.into(),
            data: data.into(),
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::StdinWritten(s) => Ok(s.bytes_written),
            _ => anyhow::bail!("unexpected response to write_stdin"),
        }
    }

    /// Read data from a process's stdout.
    pub async fn read_stdout(
        &self,
        id: impl Into<String>,
        timeout_ms: u64,
    ) -> Result<StdoutReadResponse> {
        let req = RunnerRequest::ReadStdout(ReadStdoutRequest {
            id: id.into(),
            timeout_ms,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::StdoutRead(s) => Ok(s),
            _ => anyhow::bail!("unexpected response to read_stdout"),
        }
    }

    /// Subscribe to stdout stream. Returns a stream and a reader that should be
    /// used together. The stream yields lines as they arrive from the process.
    pub async fn subscribe_stdout(&self, id: impl Into<String>) -> Result<StdoutSubscription> {
        let stream = UnixStream::connect(&self.socket_path)
            .await
            .with_context(|| format!("connecting to runner at {:?}", self.socket_path))?;

        let process_id = id.into();
        let req = RunnerRequest::SubscribeStdout(SubscribeStdoutRequest {
            id: process_id.clone(),
        });

        let (reader, mut writer) = stream.into_split();

        // Send subscription request
        let mut json = serde_json::to_string(&req).context("serializing request")?;
        json.push('\n');
        writer
            .write_all(json.as_bytes())
            .await
            .context("writing request")?;

        // Read subscription confirmation
        let reader = BufReader::new(reader);
        let mut lines = reader.lines();

        let first_line = lines
            .next_line()
            .await
            .context("reading subscription response")?
            .ok_or_else(|| anyhow::anyhow!("connection closed"))?;

        let resp: RunnerResponse = serde_json::from_str(&first_line).context("parsing response")?;

        match resp {
            RunnerResponse::StdoutSubscribed(_) => Ok(StdoutSubscription {
                lines,
                _writer: writer,
            }),
            RunnerResponse::Error(e) => {
                anyhow::bail!("runner error ({:?}): {}", e.code, e.message);
            }
            _ => anyhow::bail!("unexpected response to subscribe_stdout"),
        }
    }

    /// Kill a managed process by runner process ID.
    pub async fn kill_process(&self, id: impl Into<String>, force: bool) -> Result<bool> {
        let req = RunnerRequest::KillProcess(KillProcessRequest {
            id: id.into(),
            force,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::ProcessKilled(k) => Ok(k.was_running),
            _ => anyhow::bail!("unexpected response to kill_process"),
        }
    }
}

/// An active stdout subscription that yields lines as they arrive.
pub struct StdoutSubscription {
    lines: tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    // Keep writer alive to maintain connection
    _writer: tokio::net::unix::OwnedWriteHalf,
}

impl StdoutSubscription {
    /// Read the next event from the subscription.
    /// Returns None when the subscription ends (process exited or connection closed).
    pub async fn next(&mut self) -> Option<StdoutSubscriptionEvent> {
        match self.lines.next_line().await {
            Ok(Some(line)) => {
                match serde_json::from_str::<RunnerResponse>(&line) {
                    Ok(RunnerResponse::StdoutLine(l)) => {
                        Some(StdoutSubscriptionEvent::Line(l.line))
                    }
                    Ok(RunnerResponse::StdoutEnd(_e)) => Some(StdoutSubscriptionEvent::End),
                    Ok(_) => {
                        // Unexpected response, skip
                        None
                    }
                    Err(_) => {
                        // Parse error, skip
                        None
                    }
                }
            }
            Ok(None) | Err(_) => None,
        }
    }
}

/// Event from a stdout subscription.
#[derive(Debug, Clone)]
pub enum StdoutSubscriptionEvent {
    /// A line from stdout.
    Line(String),
    /// The subscription ended (process exited).
    End,
}

impl std::fmt::Debug for RunnerClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunnerClient")
            .field("socket_path", &self.socket_path)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default() {
        let client = RunnerClient::default();
        // Should use XDG_RUNTIME_DIR or /tmp
        let path = client.socket_path();
        assert!(path.to_string_lossy().ends_with("octo-runner.sock"));
    }

    #[test]
    fn test_custom_socket_path() {
        let client = RunnerClient::new("/tmp/test-runner.sock");
        assert_eq!(client.socket_path(), Path::new("/tmp/test-runner.sock"));
    }
}
