//! HTTP API module.
//!
//! Provides REST endpoints and proxy functionality for session management.

mod error;
mod handlers;
mod proxy;
mod routes;
mod state;

// Re-export error types for external use
#[allow(unused_imports)]
pub use error::{ApiError, ApiResult, ErrorResponse};
pub use routes::create_router;
pub use state::{AppState, MmryState, VoiceState, VisualizerVoiceState};
