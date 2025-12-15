//! Application state shared across handlers.

use std::sync::Arc;

use crate::session::SessionService;

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Session service for managing container lifecycles.
    pub sessions: Arc<SessionService>,
}

impl AppState {
    /// Create new application state.
    pub fn new(sessions: SessionService) -> Self {
        Self {
            sessions: Arc::new(sessions),
        }
    }
}
