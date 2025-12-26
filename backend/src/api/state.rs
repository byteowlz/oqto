//! Application state shared across handlers.

use std::sync::Arc;

use axum::body::Body;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

use super::super::agent::AgentService;
use crate::agent_rpc::AgentBackend;
use crate::auth::AuthState;
use crate::invite::InviteCodeRepository;
use crate::session::SessionService;
use crate::user::UserService;

/// Mmry configuration for the API layer.
#[derive(Clone, Debug)]
pub struct MmryState {
    /// Whether mmry integration is enabled.
    pub enabled: bool,
    /// Whether we're in single-user mode (proxy to local service).
    pub single_user: bool,
    /// URL of the local mmry service (for single-user mode).
    pub local_service_url: String,
}

impl Default for MmryState {
    fn default() -> Self {
        Self {
            enabled: false,
            single_user: true,
            local_service_url: "http://localhost:8081".to_string(),
        }
    }
}

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Session service for managing container lifecycles.
    pub sessions: Arc<SessionService>,
    /// Agent service for managing opencode agents within containers.
    pub agents: Arc<AgentService>,
    /// User service for user management.
    pub users: Arc<UserService>,
    /// Invite code repository for registration.
    pub invites: Arc<InviteCodeRepository>,
    /// Authentication state.
    pub auth: AuthState,
    /// HTTP client for proxying requests to per-session services.
    pub http_client: Client<HttpConnector, Body>,
    /// Unified agent backend (optional, for new AgentRPC-based architecture).
    pub agent_backend: Option<Arc<dyn AgentBackend>>,
    /// Mmry (memory service) configuration.
    pub mmry: MmryState,
}

impl AppState {
    /// Create new application state.
    pub fn new(
        sessions: SessionService,
        agents: AgentService,
        users: UserService,
        invites: InviteCodeRepository,
        auth: AuthState,
        mmry: MmryState,
    ) -> Self {
        let http_client: Client<HttpConnector, Body> =
            Client::builder(TokioExecutor::new()).build_http();

        Self {
            sessions: Arc::new(sessions),
            agents: Arc::new(agents),
            users: Arc::new(users),
            invites: Arc::new(invites),
            auth,
            http_client,
            agent_backend: None,
            mmry,
        }
    }

    /// Create new application state with AgentBackend.
    pub fn with_agent_backend(
        sessions: SessionService,
        agents: AgentService,
        users: UserService,
        invites: InviteCodeRepository,
        auth: AuthState,
        backend: Arc<dyn AgentBackend>,
        mmry: MmryState,
    ) -> Self {
        let http_client: Client<HttpConnector, Body> =
            Client::builder(TokioExecutor::new()).build_http();

        Self {
            sessions: Arc::new(sessions),
            agents: Arc::new(agents),
            users: Arc::new(users),
            invites: Arc::new(invites),
            auth,
            http_client,
            agent_backend: Some(backend),
            mmry,
        }
    }
}
