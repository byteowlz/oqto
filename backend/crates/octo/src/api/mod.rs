//! HTTP API module.
//!
//! Provides REST endpoints and proxy functionality for session management.

pub mod a2ui;
mod delegate;
mod error;
pub mod handlers;
mod main_chat;
mod main_chat_files;
mod main_chat_pi;
mod onboarding_handlers;
pub mod proxy;
mod routes;
mod state;
mod test_harness;
pub mod ui_control;
mod workspace_pi;
pub mod ws_multiplexed;

// Re-export error types for external use
#[allow(unused_imports)]
pub use error::{ApiError, ApiResult, ErrorResponse};
pub use routes::create_router_with_config;
pub use state::{
    AppState, MmryState, SessionUiState, TemplatesRepoType, TemplatesState, VisualizerVoiceState,
    VoiceState,
};
