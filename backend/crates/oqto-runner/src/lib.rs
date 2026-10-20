//! Oqto runner daemon library.
//!
//! This crate is becoming the owner of runner-daemon internals. During the
//! migration, the binary still depends on the server crate for large legacy
//! modules that have not moved yet.

pub mod agent_browser;
pub mod client;
pub mod credentials;
pub mod daemon;
pub mod endpoint_bridge;
pub mod history_import;
pub mod history_read;
#[cfg(feature = "iroh-transport")]
pub mod iroh_transport;
pub mod pair_cli;
pub mod personal_cli;
pub use daemon::server::pair_control;
mod personal_files;
mod personal_sessions;
mod personal_terminal;
pub mod pi_manager;
pub mod pi_translator;
pub mod protocol;
pub mod provider_login;
mod recovery_child;
pub mod reverse_bridge;
pub mod runtime_guard;
pub mod session_resume;
#[cfg(unix)]
mod socket_lease;
pub mod ssh_agent_proxy;
pub mod tls;
pub mod transport;
#[cfg(unix)]
pub mod tui_rpc;
pub mod wire;
