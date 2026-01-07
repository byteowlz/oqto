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
pub const DEFAULT_SOCKET_PATTERN: &str = "/run/octo/runner-{user}.sock";

/// Client for communicating with the runner daemon.
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

    /// Create a runner client for a specific user using the default socket pattern.
    pub fn for_user(username: &str) -> Self {
        let socket_path = DEFAULT_SOCKET_PATTERN.replace("{user}", username);
        Self::new(socket_path)
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

        let resp: RunnerResponse =
            serde_json::from_str(&line).context("parsing response")?;

        // Check for error response
        if let RunnerResponse::Error(e) = &resp {
            anyhow::bail!("runner error ({:?}): {}", e.code, e.message);
        }

        Ok(resp)
    }

    /// Ping the runner to check if it's alive.
    pub async fn ping(&self) -> Result<()> {
        let resp = self.request(&RunnerRequest::Ping).await?;
        match resp {
            RunnerResponse::Pong => Ok(()),
            _ => anyhow::bail!("unexpected response to ping"),
        }
    }

    /// Check if the runner is available (socket exists and responds to ping).
    pub async fn is_available(&self) -> bool {
        self.ping().await.is_ok()
    }

    /// Spawn a detached process.
    pub async fn spawn_process(
        &self,
        id: impl Into<String>,
        binary: impl Into<String>,
        args: Vec<String>,
        cwd: impl Into<PathBuf>,
        env: HashMap<String, String>,
    ) -> Result<u32> {
        let req = RunnerRequest::SpawnProcess(SpawnProcessRequest {
            id: id.into(),
            binary: binary.into(),
            args,
            cwd: cwd.into(),
            env,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::ProcessSpawned(p) => Ok(p.pid),
            _ => anyhow::bail!("unexpected response to spawn_process"),
        }
    }

    /// Spawn an RPC process with stdin/stdout pipes.
    pub async fn spawn_rpc_process(
        &self,
        id: impl Into<String>,
        binary: impl Into<String>,
        args: Vec<String>,
        cwd: impl Into<PathBuf>,
        env: HashMap<String, String>,
    ) -> Result<u32> {
        let req = RunnerRequest::SpawnRpcProcess(SpawnRpcProcessRequest {
            id: id.into(),
            binary: binary.into(),
            args,
            cwd: cwd.into(),
            env,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::ProcessSpawned(p) => Ok(p.pid),
            _ => anyhow::bail!("unexpected response to spawn_rpc_process"),
        }
    }

    /// Kill a process.
    pub async fn kill_process(&self, id: impl Into<String>, force: bool) -> Result<bool> {
        let req = RunnerRequest::KillProcess(KillProcessRequest {
            id: id.into(),
            force,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::ProcessKilled(p) => Ok(p.was_running),
            _ => anyhow::bail!("unexpected response to kill_process"),
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

    /// List all managed processes.
    pub async fn list_processes(&self) -> Result<Vec<ProcessInfo>> {
        let resp = self.request(&RunnerRequest::ListProcesses).await?;
        match resp {
            RunnerResponse::ProcessList(p) => Ok(p.processes),
            _ => anyhow::bail!("unexpected response to list_processes"),
        }
    }

    /// Write data to a process's stdin.
    pub async fn write_stdin(&self, id: impl Into<String>, data: impl Into<String>) -> Result<usize> {
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

    /// Request graceful shutdown of the runner.
    pub async fn shutdown(&self) -> Result<()> {
        let resp = self.request(&RunnerRequest::Shutdown).await?;
        match resp {
            RunnerResponse::ShuttingDown => Ok(()),
            _ => anyhow::bail!("unexpected response to shutdown"),
        }
    }
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
    fn test_for_user() {
        let client = RunnerClient::for_user("alice");
        assert_eq!(
            client.socket_path(),
            Path::new("/run/octo/runner-alice.sock")
        );
    }

    #[test]
    fn test_custom_socket_path() {
        let client = RunnerClient::new("/tmp/test-runner.sock");
        assert_eq!(client.socket_path(), Path::new("/tmp/test-runner.sock"));
    }
}
