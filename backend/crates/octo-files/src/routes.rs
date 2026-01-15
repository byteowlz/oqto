use axum::{
    Router,
    routing::{delete, get, post, put},
};

use crate::AppState;
use crate::handlers;

/// Create file server routes
pub fn file_routes() -> Router<AppState> {
    Router::new()
        // Health check
        .route("/health", get(handlers::health))
        // WebSocket file watch
        .route("/ws/watch", get(handlers::watch_ws))
        // Directory tree
        .route("/tree", get(handlers::get_tree))
        // File operations
        .route("/file", get(handlers::get_file))
        .route("/file", post(handlers::upload_file))
        .route("/file", put(handlers::write_file))
        .route("/file", delete(handlers::delete_file))
        // Directory operations
        .route("/mkdir", put(handlers::create_dir))
        // Rename/move operations
        .route("/rename", post(handlers::rename_file))
        // Download operations
        .route("/download", get(handlers::download))
        .route("/download-zip", get(handlers::download_zip))
}
