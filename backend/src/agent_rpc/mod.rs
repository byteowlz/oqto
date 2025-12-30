//! AgentRPC - Unified interface for local and container backends.
//!
//! This module defines the `AgentBackend` trait that abstracts away the difference
//! between running opencode locally (as native processes) vs in containers.
//!
//! Both backends implement the same interface, allowing octo-server to use
//! identical code paths regardless of deployment mode.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      octo-server                            │
//! │                                                             │
//! │  "Find agent endpoint for user X → call AgentBackend"       │
//! └─────────────────────┬───────────────────────────────────────┘
//!                       │
//!         ┌─────────────┴─────────────┐
//!         │                           │
//!         ▼                           ▼
//! ┌───────────────────┐     ┌───────────────────┐
//! │   Local Backend   │     │  Container Backend│
//! │                   │     │                   │
//! │ Spawns opencode   │     │ Manages container │
//! │ as native process │     │ with opencode     │
//! │                   │     │ inside            │
//! └───────────────────┘     └───────────────────┘
//! ```

mod container;
mod local;
mod types;

pub use container::{ContainerBackend, ContainerBackendConfig};
pub use local::{LocalBackend, LocalBackendConfig};
pub use types::*;

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;
use tokio_stream::Stream;
use std::pin::Pin;

/// Server-Sent Event for streaming responses.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentEvent {
    pub event_type: String,
    pub data: String,
}

/// Stream of agent events (SSE).
pub type AgentEventStream = Pin<Box<dyn Stream<Item = Result<AgentEvent>> + Send>>;

/// The AgentBackend trait defines the unified interface for interacting with
/// opencode instances, regardless of whether they run locally or in containers.
///
/// This is the core abstraction that enables multi-user support with proper
/// isolation in both local and container modes.
#[async_trait]
pub trait AgentBackend: Send + Sync {
    /// List all conversations/sessions for a user.
    ///
    /// In local mode, reads from the user's opencode storage directory.
    /// In container mode, queries the opencode API inside the container.
    async fn list_conversations(&self, user_id: &str) -> Result<Vec<Conversation>>;

    /// Get a specific conversation by ID.
    async fn get_conversation(&self, user_id: &str, conversation_id: &str) -> Result<Option<Conversation>>;

    /// Get messages for a conversation.
    async fn get_messages(&self, user_id: &str, conversation_id: &str) -> Result<Vec<Message>>;

    /// Start a new session or resume an existing one.
    ///
    /// Returns session info including the endpoint URL for the opencode API.
    ///
    /// # Arguments
    /// * `user_id` - The platform user ID
    /// * `workdir` - Working directory for the session (can use x-opencode-directory)
    /// * `opts` - Session options (model, agent, etc.)
    async fn start_session(
        &self,
        user_id: &str,
        workdir: &Path,
        opts: StartSessionOpts,
    ) -> Result<SessionHandle>;

    /// Attach to an existing session and stream events.
    ///
    /// Returns a stream of SSE events from the opencode instance.
    async fn attach(&self, user_id: &str, session_id: &str) -> Result<AgentEventStream>;

    /// Send a message to a session.
    ///
    /// The message is sent asynchronously; use `attach` to receive responses.
    async fn send_message(
        &self,
        user_id: &str,
        session_id: &str,
        message: SendMessageRequest,
    ) -> Result<()>;

    /// Stop a running session.
    ///
    /// In local mode, terminates the opencode process.
    /// In container mode, stops (but doesn't remove) the container.
    async fn stop_session(&self, user_id: &str, session_id: &str) -> Result<()>;

    /// Health check for the backend.
    ///
    /// Returns Ok if the backend is operational.
    async fn health(&self) -> Result<HealthStatus>;

    /// Get the opencode API base URL for a session.
    ///
    /// This URL can be used directly by the frontend to communicate with opencode.
    async fn get_session_url(&self, user_id: &str, session_id: &str) -> Result<Option<String>>;

    /// Get the data directory for a user's opencode storage.
    ///
    /// Used for reading chat history from disk.
    fn user_data_dir(&self, user_id: &str) -> std::path::PathBuf;
}

/// Backend mode selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackendMode {
    /// Local mode - opencode runs as native process
    Local,
    /// Container mode - opencode runs in Docker/Podman container
    #[default]
    Container,
    /// Auto mode - prefers local if available, falls back to container
    Auto,
}

impl std::fmt::Display for BackendMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendMode::Local => write!(f, "local"),
            BackendMode::Container => write!(f, "container"),
            BackendMode::Auto => write!(f, "auto"),
        }
    }
}
