//! User-plane abstraction for multi-user isolation.
//!
//! This module provides a trait that abstracts user-plane operations.
//!
//! All user data operations (filesystem, sessions, memories, main chat) go through
//! this abstraction. Production runtime is runner-mediated only.

#[cfg(test)]
mod direct;
mod metrics;
mod runner;
mod types;

pub use metrics::{MeteredUserPlane, UserPlaneMetricRow, UserPlaneMetrics, UserPlanePath};
pub use runner::RunnerUserPlane;
pub use types::*;

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

/// Trait for user-plane operations.
#[async_trait]
pub trait UserPlane: Send + Sync {
    // ========================================================================
    // Filesystem Operations
    // ========================================================================

    /// Read a file from the user's workspace.
    async fn read_file(
        &self,
        path: &Path,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> Result<FileContent>;

    /// Write a file to the user's workspace.
    async fn write_file(&self, path: &Path, content: &[u8], create_parents: bool) -> Result<()>;

    /// List contents of a directory.
    async fn list_directory(&self, path: &Path, include_hidden: bool) -> Result<Vec<DirEntry>>;

    /// Get file/directory metadata.
    async fn stat(&self, path: &Path) -> Result<FileStat>;

    /// Delete a file or directory.
    async fn delete_path(&self, path: &Path, recursive: bool) -> Result<()>;

    /// Create a directory.
    async fn create_directory(&self, path: &Path, create_parents: bool) -> Result<()>;

    /// Execute one App operation through the runner-side Gate.
    async fn run_app_operation(
        &self,
        _request: AppOperationExecution,
    ) -> Result<AppOperationExecutionResult> {
        anyhow::bail!("App operations require a runner-backed user plane")
    }

    // ========================================================================
    // Session Operations
    // ========================================================================

    /// List all sessions.
    async fn list_sessions(&self) -> Result<Vec<SessionInfo>>;

    /// Get a specific session by ID.
    async fn get_session(&self, session_id: &str) -> Result<Option<SessionInfo>>;

    /// Start services for a session.
    async fn start_session(&self, request: StartSessionRequest) -> Result<StartSessionResponse>;

    /// Stop a running session.
    async fn stop_session(&self, session_id: &str) -> Result<()>;

    // ========================================================================
    // Main Chat Operations
    // ========================================================================

    /// List main chat session files.
    async fn list_main_chat_sessions(&self) -> Result<Vec<MainChatSessionInfo>>;

    /// Get messages from a main chat session.
    async fn get_main_chat_messages(
        &self,
        session_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<MainChatMessage>>;
}
