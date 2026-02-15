//! Runner-based user-plane implementation.
//!
//! Provides user-plane access via the per-user octo-runner daemon.

use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::Engine;
use std::path::Path;

use super::UserPlane;
use super::types::*;
use crate::runner::client::RunnerClient;

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
    /// Uses the default socket path pattern: /run/user/{uid}/octo-runner.sock
    pub fn for_user(username: &str) -> Result<Self> {
        Ok(Self::new(RunnerClient::for_user(username)?))
    }

    /// Create a runner user-plane for a specific user with a custom socket pattern.
    pub fn for_user_with_pattern(username: &str, pattern: &str) -> Result<Self> {
        Ok(Self::new(RunnerClient::for_user_with_pattern(
            username, pattern,
        )?))
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
            .write_file(path, content, create_parents)
            .await
            .context("runner write_file")?;
        Ok(())
    }

    async fn list_directory(&self, path: &Path, include_hidden: bool) -> Result<Vec<DirEntry>> {
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
            .delete_path(path, recursive)
            .await
            .context("runner delete_path")?;
        Ok(())
    }

    async fn create_directory(&self, path: &Path, create_parents: bool) -> Result<()> {
        self.client
            .create_directory(path, create_parents)
            .await
            .context("runner create_directory")?;
        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
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
            .stop_session(session_id)
            .await
            .context("runner stop_session")?;
        Ok(())
    }

    async fn list_main_chat_sessions(&self) -> Result<Vec<MainChatSessionInfo>> {
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
        let response = self
            .client
            .add_memory(content, category.map(String::from), importance)
            .await
            .context("runner add_memory")?;

        Ok(response.memory_id)
    }

    async fn delete_memory(&self, memory_id: &str) -> Result<()> {
        self.client
            .delete_memory(memory_id)
            .await
            .context("runner delete_memory")?;
        Ok(())
    }
}
