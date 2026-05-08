//! Oqto runner daemon library.
//!
//! This crate is becoming the owner of runner-daemon internals. During the
//! migration, the binary still depends on the server crate for large legacy
//! modules that have not moved yet.

pub mod agent_browser;
pub mod client;
pub mod daemon;
pub mod oqto_log_projector;
pub mod pi_manager;
pub mod pi_translator;
pub mod protocol;
