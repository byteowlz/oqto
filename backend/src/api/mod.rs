//! HTTP API module.
//!
//! Provides REST endpoints and proxy functionality for session management.

mod handlers;
mod proxy;
mod routes;
mod state;

pub use routes::create_router;
pub use state::AppState;
