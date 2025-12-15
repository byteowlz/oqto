//! API route definitions.

use axum::{
    routing::{delete, get, post},
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use super::handlers;
use super::proxy;
use super::state::AppState;

/// Create the application router.
pub fn create_router(state: AppState) -> Router {
    // CORS configuration
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // Health check
        .route("/health", get(handlers::health))
        // Session management
        .route("/sessions", get(handlers::list_sessions))
        .route("/sessions", post(handlers::create_session))
        .route("/sessions/{session_id}", get(handlers::get_session))
        .route("/sessions/{session_id}", delete(handlers::delete_session))
        .route("/sessions/{session_id}/stop", post(handlers::stop_session))
        // Proxy routes
        .route(
            "/sessions/{session_id}/opencode/*path",
            get(proxy::proxy_opencode)
                .post(proxy::proxy_opencode)
                .put(proxy::proxy_opencode)
                .delete(proxy::proxy_opencode),
        )
        .route(
            "/sessions/{session_id}/files/*path",
            get(proxy::proxy_fileserver),
        )
        .route(
            "/sessions/{session_id}/terminal",
            get(proxy::proxy_terminal_ws),
        )
        // Middleware
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
