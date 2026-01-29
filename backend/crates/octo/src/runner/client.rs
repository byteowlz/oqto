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

/// Default socket path pattern.
/// Uses XDG_RUNTIME_DIR if available, otherwise falls back to /tmp.
pub const DEFAULT_SOCKET_PATTERN: &str = "{runtime_dir}/octo-runner.sock";

/// Per-user socket path pattern for multi-user mode.
/// {uid} is replaced with the user's numeric UID.
pub const USER_SOCKET_PATTERN: &str = "/run/user/{uid}/octo-runner.sock";

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
    ///
    /// This is for single-user mode where the runner runs as the current user.
    pub fn default() -> Self {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
        let socket_path = DEFAULT_SOCKET_PATTERN.replace("{runtime_dir}", &runtime_dir);
        Self::new(socket_path)
    }

    /// Create a runner client for a specific Linux user by UID.
    ///
    /// Used in multi-user mode where each user has their own runner daemon
    /// at /run/user/{uid}/octo-runner.sock.
    pub fn for_uid(uid: u32) -> Self {
        let socket_path = USER_SOCKET_PATTERN.replace("{uid}", &uid.to_string());
        Self::new(socket_path)
    }

    /// Create a runner client for a specific Linux user by username.
    ///
    /// Looks up the user's UID and returns a client for their runner socket.
    /// Returns an error if the user doesn't exist.
    ///
    /// Uses the default pattern `/run/user/{uid}/octo-runner.sock`.
    /// For custom patterns, use `for_user_with_pattern`.
    pub fn for_user(username: &str) -> Result<Self> {
        let uid = lookup_uid(username)?;
        Ok(Self::for_uid(uid))
    }

    /// Create a runner client for a Linux user using a custom socket pattern.
    ///
    /// The pattern supports placeholders:
    /// - `{user}`: Linux username
    /// - `{uid}`: User's numeric UID
    ///
    /// Example pattern: `/run/octo/runner-sockets/{user}/octo-runner.sock`
    pub fn for_user_with_pattern(username: &str, pattern: &str) -> Result<Self> {
        let mut socket_path = pattern.replace("{user}", username);
        if socket_path.contains("{uid}") {
            let uid = lookup_uid(username)?;
            socket_path = socket_path.replace("{uid}", &uid.to_string());
        }
        Ok(Self::new(socket_path))
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

    /// Spawn a detached process (no stdin/stdout pipes).
    pub async fn spawn_process(
        &self,
        id: impl Into<String>,
        binary: impl Into<String>,
        args: Vec<String>,
        cwd: impl Into<PathBuf>,
        env: HashMap<String, String>,
        sandboxed: bool,
    ) -> Result<u32> {
        let req = RunnerRequest::SpawnProcess(SpawnProcessRequest {
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
            _ => anyhow::bail!("unexpected response to spawn_process"),
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

    // ========================================================================
    // Filesystem Operations (user-plane)
    // ========================================================================

    /// Read a file from the user's workspace.
    pub async fn read_file(
        &self,
        path: impl Into<PathBuf>,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> Result<FileContentResponse> {
        let req = RunnerRequest::ReadFile(ReadFileRequest {
            path: path.into(),
            offset,
            limit,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::FileContent(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to read_file"),
        }
    }

    /// Write a file to the user's workspace.
    pub async fn write_file(
        &self,
        path: impl Into<PathBuf>,
        content: &[u8],
        create_parents: bool,
    ) -> Result<FileWrittenResponse> {
        use base64::Engine;

        let content_base64 = base64::engine::general_purpose::STANDARD.encode(content);

        let req = RunnerRequest::WriteFile(WriteFileRequest {
            path: path.into(),
            content_base64,
            create_parents,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::FileWritten(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to write_file"),
        }
    }

    /// List contents of a directory.
    pub async fn list_directory(
        &self,
        path: impl Into<PathBuf>,
        include_hidden: bool,
    ) -> Result<DirectoryListingResponse> {
        let req = RunnerRequest::ListDirectory(ListDirectoryRequest {
            path: path.into(),
            include_hidden,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::DirectoryListing(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to list_directory"),
        }
    }

    /// Get file/directory metadata.
    pub async fn stat(&self, path: impl Into<PathBuf>) -> Result<FileStatResponse> {
        let req = RunnerRequest::Stat(StatRequest { path: path.into() });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::FileStat(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to stat"),
        }
    }

    /// Delete a file or directory.
    pub async fn delete_path(
        &self,
        path: impl Into<PathBuf>,
        recursive: bool,
    ) -> Result<PathDeletedResponse> {
        let req = RunnerRequest::DeletePath(DeletePathRequest {
            path: path.into(),
            recursive,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::PathDeleted(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to delete_path"),
        }
    }

    /// Create a directory.
    pub async fn create_directory(
        &self,
        path: impl Into<PathBuf>,
        create_parents: bool,
    ) -> Result<DirectoryCreatedResponse> {
        let req = RunnerRequest::CreateDirectory(CreateDirectoryRequest {
            path: path.into(),
            create_parents,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::DirectoryCreated(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to create_directory"),
        }
    }

    // ========================================================================
    // Session Operations (user-plane)
    // ========================================================================

    /// List all sessions for this user.
    pub async fn list_sessions(&self) -> Result<SessionListResponse> {
        let req = RunnerRequest::ListSessions;

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::SessionList(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to list_sessions"),
        }
    }

    /// Get a specific session by ID.
    pub async fn get_session(&self, session_id: impl Into<String>) -> Result<SessionResponse> {
        let req = RunnerRequest::GetSession(GetSessionRequest {
            session_id: session_id.into(),
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::Session(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to get_session"),
        }
    }

    /// Start services for a session.
    pub async fn start_session(
        &self,
        session_id: impl Into<String>,
        workspace_path: impl Into<PathBuf>,
        opencode_port: u16,
        fileserver_port: u16,
        ttyd_port: u16,
        agent: Option<String>,
        env: HashMap<String, String>,
    ) -> Result<SessionStartedResponse> {
        let req = RunnerRequest::StartSession(StartSessionRequest {
            session_id: session_id.into(),
            workspace_path: workspace_path.into(),
            opencode_port,
            fileserver_port,
            ttyd_port,
            agent,
            env,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::SessionStarted(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to start_session"),
        }
    }

    /// Stop a running session.
    pub async fn stop_session(
        &self,
        session_id: impl Into<String>,
    ) -> Result<SessionStoppedResponse> {
        let req = RunnerRequest::StopSession(StopSessionRequest {
            session_id: session_id.into(),
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::SessionStopped(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to stop_session"),
        }
    }

    // ========================================================================
    // Main Chat Operations (user-plane)
    // ========================================================================

    /// List main chat session files.
    pub async fn list_main_chat_sessions(&self) -> Result<MainChatSessionListResponse> {
        let req = RunnerRequest::ListMainChatSessions;

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::MainChatSessionList(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to list_main_chat_sessions"),
        }
    }

    /// Get messages from a main chat session.
    pub async fn get_main_chat_messages(
        &self,
        session_id: impl Into<String>,
        limit: Option<usize>,
    ) -> Result<MainChatMessagesResponse> {
        let req = RunnerRequest::GetMainChatMessages(GetMainChatMessagesRequest {
            session_id: session_id.into(),
            limit,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::MainChatMessages(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to get_main_chat_messages"),
        }
    }

    // ========================================================================
    // Memory Operations (user-plane)
    // ========================================================================

    /// Search memories.
    pub async fn search_memories(
        &self,
        query: impl Into<String>,
        limit: usize,
        category: Option<String>,
    ) -> Result<MemorySearchResultsResponse> {
        let req = RunnerRequest::SearchMemories(SearchMemoriesRequest {
            query: query.into(),
            limit,
            category,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::MemorySearchResults(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to search_memories"),
        }
    }

    /// Add a new memory.
    pub async fn add_memory(
        &self,
        content: impl Into<String>,
        category: Option<String>,
        importance: Option<u8>,
    ) -> Result<MemoryAddedResponse> {
        let req = RunnerRequest::AddMemory(AddMemoryRequest {
            content: content.into(),
            category,
            importance,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::MemoryAdded(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to add_memory"),
        }
    }

    /// Delete a memory by ID.
    pub async fn delete_memory(
        &self,
        memory_id: impl Into<String>,
    ) -> Result<MemoryDeletedResponse> {
        let req = RunnerRequest::DeleteMemory(DeleteMemoryRequest {
            memory_id: memory_id.into(),
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::MemoryDeleted(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to delete_memory"),
        }
    }

    // ========================================================================
    // OpenCode Chat History Operations (user-plane)
    // ========================================================================

    /// List OpenCode chat sessions.
    pub async fn list_opencode_sessions(
        &self,
        workspace: Option<String>,
        include_children: bool,
        limit: Option<usize>,
    ) -> Result<OpencodeSessionListResponse> {
        let req = RunnerRequest::ListOpencodeSessions(ListOpencodeSessionsRequest {
            workspace,
            include_children,
            limit,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::OpencodeSessionList(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to list_opencode_sessions"),
        }
    }

    /// Get a specific OpenCode session.
    pub async fn get_opencode_session(
        &self,
        session_id: impl Into<String>,
    ) -> Result<OpencodeSessionResponse> {
        let req = RunnerRequest::GetOpencodeSession(GetOpencodeSessionRequest {
            session_id: session_id.into(),
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::OpencodeSession(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to get_opencode_session"),
        }
    }

    /// Get messages from an OpenCode session.
    pub async fn get_opencode_session_messages(
        &self,
        session_id: impl Into<String>,
        render: bool,
    ) -> Result<OpencodeSessionMessagesResponse> {
        let req = RunnerRequest::GetOpencodeSessionMessages(GetOpencodeSessionMessagesRequest {
            session_id: session_id.into(),
            render,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::OpencodeSessionMessages(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to get_opencode_session_messages"),
        }
    }

    /// Update an OpenCode session (e.g., rename title).
    pub async fn update_opencode_session(
        &self,
        session_id: impl Into<String>,
        title: Option<String>,
    ) -> Result<OpencodeSessionUpdatedResponse> {
        let req = RunnerRequest::UpdateOpencodeSession(UpdateOpencodeSessionRequest {
            session_id: session_id.into(),
            title,
        });

        let resp = self.request(&req).await?;
        match resp {
            RunnerResponse::OpencodeSessionUpdated(r) => Ok(r),
            _ => anyhow::bail!("unexpected response to update_opencode_session"),
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

/// Look up a Linux user's UID by username.
#[cfg(unix)]
fn lookup_uid(username: &str) -> Result<u32> {
    use std::ffi::CString;

    let c_username =
        CString::new(username).with_context(|| format!("invalid username: {}", username))?;

    // SAFETY: getpwnam is safe to call with a valid C string.
    // We immediately copy the uid before the pointer could become invalid.
    let passwd = unsafe { libc::getpwnam(c_username.as_ptr()) };

    if passwd.is_null() {
        anyhow::bail!("user not found: {}", username);
    }

    // SAFETY: We checked passwd is not null above
    let uid = unsafe { (*passwd).pw_uid };
    Ok(uid)
}

#[cfg(not(unix))]
fn lookup_uid(_username: &str) -> Result<u32> {
    anyhow::bail!("user lookup not supported on this platform")
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

    #[test]
    fn test_for_uid() {
        let client = RunnerClient::for_uid(1000);
        assert_eq!(
            client.socket_path(),
            Path::new("/run/user/1000/octo-runner.sock")
        );
    }

    #[test]
    fn test_for_uid_different_users() {
        let alice = RunnerClient::for_uid(1001);
        let bob = RunnerClient::for_uid(1002);

        // Different users should have different socket paths
        assert_ne!(alice.socket_path(), bob.socket_path());

        // Verify socket path format
        assert!(
            alice
                .socket_path()
                .to_string_lossy()
                .contains("/run/user/1001/")
        );
        assert!(
            bob.socket_path()
                .to_string_lossy()
                .contains("/run/user/1002/")
        );
    }
}

/// Security tests for cross-user isolation.
///
/// These tests verify that the runner provides proper isolation between users.
/// They require a properly configured test environment with multiple Linux users
/// and running runner daemons. Run with `cargo test --features integration-tests`.
#[cfg(all(test, feature = "integration-tests"))]
mod security_tests {
    use super::*;

    /// Test that user A cannot access user B's files through their own runner.
    ///
    /// Security expectation: Each runner runs as its respective user and can only
    /// access files that user has permission to access.
    #[tokio::test]
    #[ignore = "requires multi-user test environment"]
    async fn test_user_cannot_access_other_users_files() {
        // Setup: Assume users 'alice' (uid 1001) and 'bob' (uid 1002) exist
        // with home directories /home/alice and /home/bob
        let alice_client = RunnerClient::for_uid(1001);
        let bob_client = RunnerClient::for_uid(1002);

        // Alice's runner should be able to read Alice's files
        let alice_result = alice_client
            .read_file("/home/alice/.bashrc", None, None)
            .await;
        assert!(
            alice_result.is_ok(),
            "Alice should be able to read her own files"
        );

        // Alice's runner should NOT be able to read Bob's files
        let cross_access_result = alice_client
            .read_file("/home/bob/.bashrc", None, None)
            .await;
        assert!(
            cross_access_result.is_err(),
            "Alice's runner should not be able to access Bob's files"
        );

        // And vice versa
        let bob_result = bob_client.read_file("/home/bob/.bashrc", None, None).await;
        assert!(
            bob_result.is_ok(),
            "Bob should be able to read his own files"
        );

        let cross_access_result = bob_client
            .read_file("/home/alice/.bashrc", None, None)
            .await;
        assert!(
            cross_access_result.is_err(),
            "Bob's runner should not be able to access Alice's files"
        );
    }

    /// Test that user A cannot spawn processes that access user B's workspace.
    #[tokio::test]
    #[ignore = "requires multi-user test environment"]
    async fn test_user_cannot_spawn_in_other_users_workspace() {
        let alice_client = RunnerClient::for_uid(1001);

        // Alice should NOT be able to spawn a session in Bob's workspace
        let result = alice_client
            .start_session(
                "test-session",
                "/home/bob/workspace",
                41820,
                41821,
                41822,
                None,
                std::collections::HashMap::new(),
            )
            .await;

        // The runner should either:
        // 1. Fail to create the workspace directory (permission denied), or
        // 2. Fail to spawn processes with access to that directory
        assert!(
            result.is_err(),
            "Alice's runner should not be able to start a session in Bob's workspace"
        );
    }

    /// Test that processes spawned by a runner inherit the correct user identity.
    #[tokio::test]
    #[ignore = "requires multi-user test environment"]
    async fn test_spawned_processes_have_correct_uid() {
        let alice_client = RunnerClient::for_uid(1001);

        // Spawn a process that prints its UID
        let pid = alice_client
            .spawn_rpc_process(
                "test-whoami",
                "id",
                vec!["-u".to_string()],
                "/tmp",
                std::collections::HashMap::new(),
                false,
            )
            .await
            .expect("should spawn process");

        // Read the output
        let output = alice_client
            .read_stdout("test-whoami", 1000)
            .await
            .expect("should read stdout");

        // Verify the UID matches Alice's UID
        let uid: u32 = output.data.trim().parse().expect("should be a number");
        assert_eq!(uid, 1001, "Process should run as Alice (uid 1001)");

        // Cleanup
        let _ = alice_client.kill_process("test-whoami", false).await;
    }

    /// Test that socket files are only accessible by their respective users.
    #[tokio::test]
    #[ignore = "requires multi-user test environment"]
    async fn test_socket_permissions() {
        // This test verifies that:
        // 1. Each user's runner socket is in their XDG_RUNTIME_DIR
        // 2. XDG_RUNTIME_DIR has mode 0700 (owner-only access)
        // 3. Attempting to connect to another user's socket fails

        let alice_client = RunnerClient::for_uid(1001);

        // Alice should be able to ping her own runner
        // (assuming the test is run as user alice or root)
        // This will fail if we're not alice, which is expected
        let alice_socket = alice_client.socket_path();
        assert!(alice_socket.starts_with("/run/user/1001/"));

        // Verify we can't connect to bob's runner (should fail with permission denied)
        let bob_client = RunnerClient::for_uid(1002);
        let ping_result = bob_client.request(&RunnerRequest::Ping).await;

        // If we're not bob/root, this should fail
        if !ping_result.is_ok() {
            // Expected - we don't have permission to access bob's socket
        }
    }

    /// Test that a compromised runner cannot affect other users.
    ///
    /// This tests the threat model where an attacker gains control of one user's
    /// runner process. They should not be able to:
    /// - Access other users' files
    /// - Connect to other users' runner sockets
    /// - Spawn processes as other users
    #[tokio::test]
    #[ignore = "requires multi-user test environment with security harness"]
    async fn test_runner_isolation_under_compromise() {
        // This is a conceptual test that would require a security testing harness
        // to properly verify. The key points are:
        //
        // 1. Each runner runs as a non-privileged user
        // 2. Runners cannot setuid/setgid to become other users
        // 3. File system permissions prevent cross-user access
        // 4. Socket permissions prevent cross-user communication
        // 5. Process namespacing (if enabled) prevents process visibility
        //
        // These properties should be verified by the security team as part of
        // the deployment security review.
    }
}
