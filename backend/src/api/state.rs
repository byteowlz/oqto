//! Application state shared across handlers.

use std::sync::Arc;

use crate::auth::AuthState;
use crate::session::SessionService;
use crate::user::UserService;

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Session service for managing container lifecycles.
    pub sessions: Arc<SessionService>,
    /// User service for user management.
    pub users: Arc<UserService>,
    /// Authentication state.
    pub auth: AuthState,
}

impl AppState {
    /// Create new application state.
    pub fn new(sessions: SessionService, users: UserService, auth: AuthState) -> Self {
        Self {
            sessions: Arc::new(sessions),
            users: Arc::new(users),
            auth,
        }
    }
}
