//! WebSocket hub for unified real-time communication.
//!
//! This module provides a centralized WebSocket hub that manages connections
//! between frontend clients and backend agent runtimes.

pub mod hub;
pub mod types;

pub use hub::WsHub;
pub use types::{UiSpotlightStep, WsCommand, WsEvent};
