//! Transitional module layout for multiplexed websocket handling.
//!
//! This scaffolds channel-scoped modules while preserving the existing
//! `api::ws_multiplexed` implementation as the single runtime source.

pub mod agent;
pub mod files;
pub mod history;
pub mod system;
pub mod terminal;

pub use crate::api::ws_multiplexed::{WsMultiplexedQuery, ws_multiplexed_handler};
