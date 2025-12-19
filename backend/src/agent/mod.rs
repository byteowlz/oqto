//! Agent management module.
//!
//! Manages multiple opencode agent instances within a container.
//! Each agent runs in its own subdirectory with its own AGENTS.md instructions.

mod models;
mod service;

pub use models::*;
pub use service::*;
