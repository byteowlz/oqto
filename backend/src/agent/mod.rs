//! Agent management module.
//!
//! Manages multiple opencode agent instances within a container.
//! Each agent runs in its own subdirectory with its own AGENTS.md instructions.

mod models;
mod repository;
mod service;

pub use models::*;
#[allow(unused_imports)]
pub use repository::{AgentRecord, AgentRepository};
pub use service::*;
