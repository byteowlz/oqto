//! Canonical protocol types for Oqto agent communication.
//!
//! This crate defines the canonical message, event, and command formats used across
//! all Oqto communication boundaries:
//!
//! ```text
//! Frontend <--[WS: canonical events/commands]--> Backend <--[canonical stream]--> Runner(s)
//!                                                                                    |
//!                                                                              Agent Harness
//!                                                                              (pi, opencode, ...)
//! ```
//!
//! The frontend speaks only the canonical protocol. It does not know or care which
//! agent harness is running. The runner translates native agent events into canonical
//! format.
//!
//! ## Design Principles
//!
//! 1. **Timeline is authoritative.** oqto-log stores the lossless, tree-aware
//!    canonical timeline.
//! 2. **Messages are a view, events are ephemeral.** Chat messages are projected
//!    from timeline turns. Events drive the UI but are not the durable authority.
//! 3. **Parts are the atomic content unit.** Oqto owns the wire type in `parts`.
//! 4. **Events form a state machine.** The frontend can derive UI state from any single event.
//! 5. **Agent-agnostic.** Supports any harness. Agent-specific features use `x-*` extensions.

pub mod canon;
pub mod commands;
pub mod delegation;
pub mod events;
pub mod messages;
pub mod parts;
pub mod projection;
pub mod runner;
pub mod timeline;

// Re-export canonical part types as the top-level content unit.
pub use parts::{FileRange, MediaSource, Part, Sender, SenderType, ToolStatus};
