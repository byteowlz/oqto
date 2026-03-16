//! Oqto Event Bus — scoped pub/sub for apps, agents, and system events.
//!
//! The bus is an in-memory event fabric layered on the existing WS mux.
//! It supports three scopes (session, workspace, global) with server-enforced
//! authorization on every publish and subscribe operation.
//!
//! Design: docs/design/unified-event-bus-and-agent-ui.md

pub mod engine;
pub mod types;

pub use engine::{BusEngine, SubscriberId};
pub use types::*;
