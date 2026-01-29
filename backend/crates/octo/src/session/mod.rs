//! Session management module.
//!
//! Handles the lifecycle of container sessions including creation,
//! monitoring, and cleanup.

mod models;
mod repository;
mod service;

#[allow(unused_imports)]
pub use models::SessionStatus;
pub use models::{CreateSessionRequest, RuntimeMode, Session, SessionResponse, SessionUrls};
pub use repository::SessionRepository;
#[allow(unused_imports)]
pub use service::{
    ContainerStatsReport, SessionContainerStats, SessionService, SessionServiceConfig,
};
