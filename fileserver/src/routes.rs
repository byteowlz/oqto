use axum::{
    routing::{delete, get, post, put},
    Router,
};

use crate::handlers;
use crate::AppState;

/// Create file server routes
pub fn file_routes() -> Router<AppState> {
    Router::new()
        // Health check
        .route("/health", get(handlers::health))
        // Directory tree
        .route("/tree", get(handlers::get_tree))
        // File operations
        .route("/file", get(handlers::get_file))
        .route("/file", post(handlers::upload_file))
        .route("/file", delete(handlers::delete_file))
        // Directory operations
        .route("/mkdir", put(handlers::create_dir))
}
