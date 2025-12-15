//! Session management module.
//!
//! Handles the lifecycle of container sessions including creation,
//! monitoring, and cleanup.

mod models;
mod repository;
mod service;

pub use models::{Session, SessionConfig, SessionStatus, CreateSessionRequest};
pub use repository::SessionRepository;
pub use service::{SessionService, SessionServiceConfig};
