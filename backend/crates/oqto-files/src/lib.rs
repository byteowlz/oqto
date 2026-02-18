//! File server library for workspace file access.
//!
//! This crate provides handlers and routes for serving files from a workspace directory.
//! It can be used as a standalone binary or embedded in another application.

pub mod config;
pub mod error;
pub mod handlers;
pub mod routes;

use std::path::PathBuf;
use std::sync::Arc;

pub use config::Config;
pub use error::FileServerError;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    /// Root directory to serve files from
    pub root_dir: PathBuf,
    /// Configuration
    pub config: Arc<Config>,
}

impl AppState {
    /// Create a new AppState with the given root directory and default config.
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            root_dir,
            config: Arc::new(Config::default()),
        }
    }

    /// Create a new AppState with the given root directory and config.
    pub fn with_config(root_dir: PathBuf, config: Config) -> Self {
        Self {
            root_dir,
            config: Arc::new(config),
        }
    }
}
