//! Session management module.
//!
//! Handles the lifecycle of container sessions including creation,
//! monitoring, and cleanup.

mod models;
mod repository;
mod service;
mod workspace_locations;

#[allow(unused_imports)]
pub use models::SessionStatus;
#[allow(unused_imports)]
pub use models::{CreateSessionRequest, RuntimeMode, Session, SessionResponse, SessionUrls};
pub use repository::SessionRepository;
#[allow(unused_imports)]
pub use service::{
    BrowserAction, ContainerStatsReport, SessionContainerStats, SessionService,
    SessionServiceConfig,
};
pub use workspace_locations::{
    WorkspaceLocation, WorkspaceLocationInput, WorkspaceLocationRepository,
};
