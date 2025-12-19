//! Application state shared across handlers.

use std::sync::Arc;

use axum::body::Body;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

use super::super::agent::AgentService;
use crate::auth::AuthState;
use crate::invite::InviteCodeRepository;
use crate::session::SessionService;
use crate::user::UserService;

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
}

impl AppState {
    /// Create new application state.
    pub fn new(
        sessions: SessionService,
        agents: AgentService,
        users: UserService,
        invites: InviteCodeRepository,
        auth: AuthState,
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
        }
    }
}
