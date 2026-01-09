//! WebSocket hub for unified real-time communication.
//!
//! This module provides a centralized WebSocket hub that manages connections
//! between frontend clients and backend agent runtimes (OpenCode, Pi).
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                          Frontend (React)                               │
//! │  - Single WebSocket connection per user                                 │
//! │  - Receives unified WsEvent stream                                      │
//! │  - Sends WsCommand for actions                                          │
//! └─────────────────────────────────────────┬───────────────────────────────┘
//!                                           │ WebSocket
//!                                           │
//! ┌─────────────────────────────────────────▼───────────────────────────────┐
//! │                          WebSocket Hub                                  │
//! │  - Per-user connection management                                       │
//! │  - Session multiplexing (events tagged with session_id)                 │
//! │  - Broadcast channel for events                                         │
//! └─────────────────────────────────────────┬───────────────────────────────┘
//!                                           │
//!           ┌───────────────────────────────┼───────────────────────────────┐
//!           │                               │                               │
//! ┌─────────▼─────────┐           ┌─────────▼─────────┐           ┌─────────▼─────────┐
//! │  OpenCodeAdapter  │           │    PiAdapter      │           │  Future Adapters  │
//! │  (SSE client)     │           │  (stdin/stdout)   │           │                   │
//! └───────────────────┘           └───────────────────┘           └───────────────────┘
//! ```

mod types;
mod hub;
mod handler;
mod opencode_adapter;

pub use hub::WsHub;
pub use handler::ws_handler;
