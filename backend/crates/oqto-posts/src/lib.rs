//! Runner-owned posts. Socket identity is authority; client fields are claims.
//! This crate never grants permissions and never writes Session history.

pub mod reply_sink;
#[cfg(unix)]
pub mod server;
#[cfg(unix)]
pub mod service;
pub mod store;
pub mod wire;
