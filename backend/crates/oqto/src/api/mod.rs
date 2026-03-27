//! HTTP API module.
//!
//! Provides REST endpoints and proxy functionality for session management.

pub mod a2ui;
mod audit;
mod delegate;
mod error;
pub mod handlers;
mod onboarding_handlers;
pub mod proxy;
mod routes;
mod state;
mod test_harness;
pub mod ui_control;
pub mod ws_multiplexed;

// Re-export error types for external use
#[allow(unused_imports)]
pub use error::{ApiError, ApiResult, ErrorResponse};
pub use routes::{create_admin_router_with_config, create_router_with_config};
pub use state::{
    AppState, EavsConfigPaths, MmryState, SessionUiState, TemplatesRepoType, TemplatesState,
    VisualizerVoiceState, VoiceState,
};
