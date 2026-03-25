//! Runner-based user-plane implementation.
//!
//! Provides user-plane access via the per-user oqto-runner daemon.

use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::Engine;
use std::path::Path;

use super::UserPlane;
use super::types::*;
use crate::runner::client::{RunnerClient, RunnerEndpointPattern};

/// Runner-based user-plane implementation.
///
/// All operations are proxied through the per-user runner daemon,
/// ensuring the backend cannot directly access user data.
#[derive(Debug, Clone)]
pub struct RunnerUserPlane {
    /// Runner client for this user.
    client: RunnerClient,
}

impl RunnerUserPlane {
    /// Create a new runner user-plane with the given client.
    pub fn new(client: RunnerClient) -> Self {
        Self { client }
    }

    /// Create a runner user-plane for a specific user.
    ///
    /// Uses the default socket path pattern: /run/user/{uid}/oqto-runner.sock
    pub fn for_user(username: &str) -> Result<Self> {
        Ok(Self::new(RunnerClient::for_user(username)?))
    }

    /// Create a runner user-plane for a specific user with a custom socket pattern.
    pub fn for_user_with_pattern(username: &str, pattern: &str) -> Result<Self> {
        Ok(Self::new(RunnerClient::for_user_with_pattern(
            username, pattern,
        )?))
    }

    /// Create a runner user-plane for a specific user with a structured endpoint template.
    pub fn for_user_with_endpoint(
        username: &str,
        endpoint: &RunnerEndpointPattern,
    ) -> Result<Self> {
        Ok(Self::new(RunnerClient::for_user_with_endpoint(
            username, endpoint,
        )?))
    }

    /// Create a runner user-plane using the default socket path.
    ///
    /// Verifies the socket exists before returning. Used in single-user mode
    /// to opportunistically route through the runner when available.
    pub fn new_default() -> Result<Self> {
        let client = RunnerClient::default();
        let path = client.socket_path();
        if !path.exists() {
            anyhow::bail!("runner socket not found at {:?}", path);
        }
        Ok(Self::new(client))
    }
}

/// Create a runner user-plane using the default socket path.
impl Default for RunnerUserPlane {
    fn default() -> Self {
        Self::new(RunnerClient::default())
    }
}

#[async_trait]
impl UserPlane for RunnerUserPlane {
    async fn read_file(
        &self,
        path: &Path,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> Result<FileContent> {
        self.client
            .ensure_ready_with_recovery()
            .await
            .context("runner readiness")?;

        let response = self
            .client
            .read_file(path, offset, limit)
            .await
            .context("runner read_file")?;

        let content = base64::engine::general_purpose::STANDARD
            .decode(&response.content_base64)
            .context("decoding base64 content")?;

        Ok(FileContent {
            content,
            size: response.size,
            truncated: response.truncated,
        })
    }

    async fn write_file(&self, path: &Path, content: &[u8], create_parents: bool) -> Result<()> {
        self.client
            .ensure_ready_with_recovery()
            .await
            .context("runner readiness")?;
        self.client
            .write_file(path, content, create_parents)
            .await
            .context("runner write_file")?;
        Ok(())
    }

    async fn list_directory(&self, path: &Path, include_hidden: bool) -> Result<Vec<DirEntry>> {
        self.client
            .ensure_ready_with_recovery()
            .await
            .context("runner readiness")?;
        let response = self
            .client
            .list_directory(path, include_hidden)
            .await
            .context("runner list_directory")?;

        // Convert from protocol types to user_plane types
        Ok(response
            .entries
            .into_iter()
            .map(|e| DirEntry {
                name: e.name,
                is_dir: e.is_dir,
                is_symlink: e.is_symlink,
                size: e.size,
                modified_at: e.modified_at,
            })
            .collect())
    }

    async fn stat(&self, path: &Path) -> Result<FileStat> {
        self.client
            .ensure_ready_with_recovery()
            .await
            .context("runner readiness")?;
        let response = self.client.stat(path).await.context("runner stat")?;

        Ok(FileStat {
            exists: response.exists,
            is_file: response.is_file,
            is_dir: response.is_dir,
            is_symlink: response.is_symlink,
            size: response.size,
            modified_at: response.modified_at,
            created_at: response.created_at,
            mode: response.mode,
        })
    }

    async fn delete_path(&self, path: &Path, recursive: bool) -> Result<()> {
        self.client
            .ensure_ready_with_recovery()
            .await
            .context("runner readiness")?;
        self.client
            .delete_path(path, recursive)
            .await
            .context("runner delete_path")?;
        Ok(())
    }

    async fn create_directory(&self, path: &Path, create_parents: bool) -> Result<()> {
        self.client
            .ensure_ready_with_recovery()
            .await
            .context("runner readiness")?;
        self.client
            .create_directory(path, create_parents)
            .await
            .context("runner create_directory")?;
        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        self.client
            .ensure_ready_with_recovery()
            .await
            .context("runner readiness")?;
        let response = self
            .client
            .list_sessions()
            .await
            .context("runner list_sessions")?;

        Ok(response
            .sessions
            .into_iter()
            .map(|s| SessionInfo {
                id: s.id,
                workspace_path: s.workspace_path,
                status: s.status,
                agent_port: s.agent_port,
                fileserver_port: s.fileserver_port,
                ttyd_port: s.ttyd_port,
                pids: s.pids,
                created_at: s.created_at,
                started_at: s.started_at,
                last_activity_at: s.last_activity_at,
            })
            .collect())
    }

    async fn get_session(&self, session_id: &str) -> Result<Option<SessionInfo>> {
        self.client
            .ensure_ready_with_recovery()
            .await
            .context("runner readiness")?;
        let response = self
            .client
            .get_session(session_id)
            .await
            .context("runner get_session")?;

        Ok(response.session.map(|s| SessionInfo {
            id: s.id,
            workspace_path: s.workspace_path,
            status: s.status,
            agent_port: s.agent_port,
            fileserver_port: s.fileserver_port,
            ttyd_port: s.ttyd_port,
            pids: s.pids,
            created_at: s.created_at,
            started_at: s.started_at,
            last_activity_at: s.last_activity_at,
        }))
    }

    async fn start_session(&self, request: StartSessionRequest) -> Result<StartSessionResponse> {
        self.client
            .ensure_ready_with_recovery()
            .await
            .context("runner readiness")?;
        let response = self
            .client
            .start_session(
                &request.session_id,
                &request.workspace_path,
                request.agent_port,
                request.fileserver_port,
                request.ttyd_port,
                request.agent,
                request.env,
            )
            .await
            .context("runner start_session")?;

        Ok(StartSessionResponse {
            session_id: response.session_id,
            pids: response.pids,
        })
    }

    async fn stop_session(&self, session_id: &str) -> Result<()> {
        self.client
            .ensure_ready_with_recovery()
            .await
            .context("runner readiness")?;
        self.client
            .stop_session(session_id)
            .await
            .context("runner stop_session")?;
        Ok(())
    }

    async fn list_main_chat_sessions(&self) -> Result<Vec<MainChatSessionInfo>> {
        self.client
            .ensure_ready_with_recovery()
            .await
            .context("runner readiness")?;
        let response = self
            .client
            .list_main_chat_sessions()
            .await
            .context("runner list_main_chat_sessions")?;

        Ok(response
            .sessions
            .into_iter()
            .map(|s| MainChatSessionInfo {
                id: s.id,
                title: s.title,
                message_count: s.message_count,
                size: s.size,
                modified_at: s.modified_at,
                started_at: s.started_at,
            })
            .collect())
    }

    async fn get_main_chat_messages(
        &self,
        session_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<MainChatMessage>> {
        self.client
            .ensure_ready_with_recovery()
            .await
            .context("runner readiness")?;
        let response = self
            .client
            .get_main_chat_messages(session_id, limit)
            .await
            .context("runner get_main_chat_messages")?;

        Ok(response
            .messages
            .into_iter()
            .map(|m| MainChatMessage {
                id: m.id,
                role: m.role,
                content: m.content,
                timestamp: m.timestamp,
            })
            .collect())
    }

    async fn search_memories(
        &self,
        query: &str,
        limit: usize,
        category: Option<&str>,
    ) -> Result<MemorySearchResults> {
        self.client
            .ensure_ready_with_recovery()
            .await
            .context("runner readiness")?;
        let response = self
            .client
            .search_memories(query, limit, category.map(String::from))
            .await
            .context("runner search_memories")?;

        Ok(MemorySearchResults {
            memories: response
                .memories
                .into_iter()
                .map(|m| MemoryEntry {
                    id: m.id,
                    content: m.content,
                    category: m.category,
                    importance: m.importance,
                    created_at: m.created_at,
                    score: m.score,
                })
                .collect(),
            total: response.total,
        })
    }

    async fn add_memory(
        &self,
        content: &str,
        category: Option<&str>,
        importance: Option<u8>,
    ) -> Result<String> {
        self.client
            .ensure_ready_with_recovery()
            .await
            .context("runner readiness")?;
        let response = self
            .client
            .add_memory(content, category.map(String::from), importance)
            .await
            .context("runner add_memory")?;

        Ok(response.memory_id)
    }

    async fn delete_memory(&self, memory_id: &str) -> Result<()> {
        self.client
            .ensure_ready_with_recovery()
            .await
            .context("runner readiness")?;
        self.client
            .delete_memory(memory_id)
            .await
            .context("runner delete_memory")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::runner::protocol::{
        DirEntry, DirectoryCreatedResponse, DirectoryListingResponse, FileContentResponse,
        FileStatResponse, FileWrittenResponse, GetSessionRequest, ListDirectoryRequest,
        MainChatMessage, MainChatMessagesResponse, MainChatSessionInfo,
        MainChatSessionListResponse, MemoryAddedResponse, MemoryDeletedResponse, MemoryEntry,
        MemorySearchResultsResponse, PathDeletedResponse, RunnerCapabilitiesResponse,
        RunnerFeatureFlags, RunnerRequest, RunnerResponse, SessionInfo, SessionListResponse,
        SessionResponse, SessionStartedResponse, SessionStoppedResponse,
        StartSessionRequest as RunnerStartSessionRequest, StatRequest, StopSessionRequest,
        WriteFileRequest,
    };
    use tempfile::tempdir;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;

    async fn spawn_mock_runner(socket_path: PathBuf) -> tokio::task::JoinHandle<()> {
        let _ = std::fs::remove_file(&socket_path);
        let listener = UnixListener::bind(&socket_path).expect("bind mock runner socket");

        tokio::spawn(async move {
            loop {
                let (mut stream, _) = match listener.accept().await {
                    Ok(conn) => conn,
                    Err(_) => break,
                };

                let mut line = String::new();
                let mut reader = BufReader::new(&mut stream);
                if reader
                    .read_line(&mut line)
                    .await
                    .ok()
                    .filter(|n| *n > 0)
                    .is_none()
                {
                    continue;
                }

                let req: RunnerRequest = match serde_json::from_str(&line) {
                    Ok(req) => req,
                    Err(_) => continue,
                };

                let response = match req {
                    RunnerRequest::Ping => RunnerResponse::Pong,
                    RunnerRequest::GetCapabilities => {
                        RunnerResponse::RunnerCapabilities(RunnerCapabilitiesResponse {
                            harnesses: vec!["pi".to_string()],
                            features: RunnerFeatureFlags {
                                command_discovery: true,
                                model_discovery: true,
                                fork: true,
                                extension_ui: true,
                            },
                        })
                    }
                    RunnerRequest::ListSessions => {
                        RunnerResponse::SessionList(SessionListResponse {
                            sessions: vec![SessionInfo {
                                id: "ses-1".to_string(),
                                workspace_path: PathBuf::from("/tmp/workspace"),
                                status: "running".to_string(),
                                agent_port: Some(4100),
                                fileserver_port: Some(4101),
                                ttyd_port: Some(4102),
                                pids: Some("123,124".to_string()),
                                created_at: "2026-03-17T00:00:00Z".to_string(),
                                started_at: Some("2026-03-17T00:00:01Z".to_string()),
                                last_activity_at: Some("2026-03-17T00:00:02Z".to_string()),
                            }],
                        })
                    }
                    RunnerRequest::GetSession(GetSessionRequest { session_id }) => {
                        RunnerResponse::Session(SessionResponse {
                            session: (session_id == "ses-1").then_some(SessionInfo {
                                id: "ses-1".to_string(),
                                workspace_path: PathBuf::from("/tmp/workspace"),
                                status: "running".to_string(),
                                agent_port: Some(4100),
                                fileserver_port: Some(4101),
                                ttyd_port: Some(4102),
                                pids: Some("123,124".to_string()),
                                created_at: "2026-03-17T00:00:00Z".to_string(),
                                started_at: Some("2026-03-17T00:00:01Z".to_string()),
                                last_activity_at: Some("2026-03-17T00:00:02Z".to_string()),
                            }),
                        })
                    }
                    RunnerRequest::StartSession(RunnerStartSessionRequest {
                        session_id, ..
                    }) => RunnerResponse::SessionStarted(SessionStartedResponse {
                        session_id,
                        pids: "123,124".to_string(),
                    }),
                    RunnerRequest::StopSession(StopSessionRequest { .. }) => {
                        RunnerResponse::SessionStopped(SessionStoppedResponse {
                            session_id: "ses-1".to_string(),
                        })
                    }
                    RunnerRequest::ReadFile(req) => {
                        RunnerResponse::FileContent(FileContentResponse {
                            path: req.path,
                            content_base64: base64::engine::general_purpose::STANDARD
                                .encode("hello from runner"),
                            size: 17,
                            truncated: false,
                        })
                    }
                    RunnerRequest::WriteFile(WriteFileRequest {
                        path,
                        content_base64,
                        ..
                    }) => {
                        let bytes_written = base64::engine::general_purpose::STANDARD
                            .decode(content_base64)
                            .map(|v| v.len() as u64)
                            .unwrap_or(0);
                        RunnerResponse::FileWritten(FileWrittenResponse {
                            path,
                            bytes_written,
                        })
                    }
                    RunnerRequest::ListDirectory(ListDirectoryRequest { path, .. }) => {
                        RunnerResponse::DirectoryListing(DirectoryListingResponse {
                            path,
                            entries: vec![DirEntry {
                                name: "src".to_string(),
                                is_dir: true,
                                is_symlink: false,
                                size: 0,
                                modified_at: 123,
                            }],
                        })
                    }
                    RunnerRequest::Stat(StatRequest { path }) => {
                        RunnerResponse::FileStat(FileStatResponse {
                            path,
                            exists: true,
                            is_file: true,
                            is_dir: false,
                            is_symlink: false,
                            size: 42,
                            modified_at: 123,
                            created_at: Some(100),
                            mode: 0o644,
                        })
                    }
                    RunnerRequest::DeletePath(req) => {
                        RunnerResponse::PathDeleted(PathDeletedResponse { path: req.path })
                    }
                    RunnerRequest::CreateDirectory(req) => {
                        RunnerResponse::DirectoryCreated(DirectoryCreatedResponse {
                            path: req.path,
                        })
                    }
                    RunnerRequest::SearchMemories(req) => {
                        RunnerResponse::MemorySearchResults(MemorySearchResultsResponse {
                            query: req.query,
                            memories: vec![MemoryEntry {
                                id: "mem-1".to_string(),
                                content: "runner memory".to_string(),
                                category: Some("backend".to_string()),
                                importance: Some(7),
                                created_at: "2026-03-17T00:00:00Z".to_string(),
                                score: Some(0.99),
                            }],
                            total: 1,
                        })
                    }
                    RunnerRequest::AddMemory(_) => {
                        RunnerResponse::MemoryAdded(MemoryAddedResponse {
                            memory_id: "mem-1".to_string(),
                        })
                    }
                    RunnerRequest::DeleteMemory(req) => {
                        RunnerResponse::MemoryDeleted(MemoryDeletedResponse {
                            memory_id: req.memory_id,
                        })
                    }
                    RunnerRequest::ListMainChatSessions => {
                        RunnerResponse::MainChatSessionList(MainChatSessionListResponse {
                            sessions: vec![MainChatSessionInfo {
                                id: "chat-1".to_string(),
                                title: Some("Chat".to_string()),
                                message_count: 1,
                                size: 10,
                                modified_at: 123,
                                started_at: "2026-03-17T00:00:00Z".to_string(),
                            }],
                        })
                    }
                    RunnerRequest::GetMainChatMessages(req) => {
                        RunnerResponse::MainChatMessages(MainChatMessagesResponse {
                            session_id: req.session_id,
                            messages: vec![MainChatMessage {
                                id: "msg-1".to_string(),
                                role: "assistant".to_string(),
                                content: serde_json::json!({"text":"ok"}),
                                timestamp: 123,
                            }],
                        })
                    }
                    _ => RunnerResponse::Error(crate::runner::protocol::ErrorResponse {
                        code: crate::runner::protocol::ErrorCode::InvalidRequest,
                        message: "unsupported request in mock runner".to_string(),
                    }),
                };

                let payload = serde_json::to_string(&response).expect("serialize response");
                let _ = stream.write_all(payload.as_bytes()).await;
                let _ = stream.write_all(b"\n").await;
            }
        })
    }

    #[tokio::test]
    async fn integration_session_lifecycle_via_user_plane() {
        let temp = tempdir().expect("tempdir");
        let socket_path = temp.path().join("oqto-runner.sock");
        let server = spawn_mock_runner(socket_path.clone()).await;

        let up = RunnerUserPlane::new(RunnerClient::new(&socket_path));

        let sessions = up.list_sessions().await.expect("list sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "ses-1");

        let session = up.get_session("ses-1").await.expect("get session");
        assert!(session.is_some());

        let started = up
            .start_session(StartSessionRequest {
                session_id: "ses-1".to_string(),
                workspace_path: PathBuf::from("/tmp/workspace"),
                agent_port: 4100,
                fileserver_port: 4101,
                ttyd_port: 4102,
                agent: Some("pi".to_string()),
                env: HashMap::new(),
            })
            .await
            .expect("start session");
        assert_eq!(started.session_id, "ses-1");

        up.stop_session("ses-1").await.expect("stop session");

        server.abort();
    }

    #[tokio::test]
    async fn integration_file_and_memory_ops_via_user_plane() {
        let temp = tempdir().expect("tempdir");
        let socket_path = temp.path().join("oqto-runner.sock");
        let server = spawn_mock_runner(socket_path.clone()).await;

        let up = RunnerUserPlane::new(RunnerClient::new(&socket_path));

        let file = up
            .read_file(Path::new("/tmp/workspace/README.md"), None, None)
            .await
            .expect("read file");
        assert_eq!(String::from_utf8_lossy(&file.content), "hello from runner");

        up.write_file(Path::new("/tmp/workspace/new.txt"), b"abc", true)
            .await
            .expect("write file");

        let entries = up
            .list_directory(Path::new("/tmp/workspace"), false)
            .await
            .expect("list directory");
        assert_eq!(entries[0].name, "src");

        let stat = up
            .stat(Path::new("/tmp/workspace/new.txt"))
            .await
            .expect("stat");
        assert!(stat.exists);

        up.create_directory(Path::new("/tmp/workspace/dir"), true)
            .await
            .expect("mkdir");
        up.delete_path(Path::new("/tmp/workspace/dir"), true)
            .await
            .expect("delete");

        let memory = up
            .search_memories("runner", 5, Some("backend"))
            .await
            .expect("search memories");
        assert_eq!(memory.total, 1);

        let memory_id = up
            .add_memory("runner memory", Some("backend"), Some(7))
            .await
            .expect("add memory");
        assert_eq!(memory_id, "mem-1");

        up.delete_memory("mem-1").await.expect("delete memory");

        server.abort();
    }
}
