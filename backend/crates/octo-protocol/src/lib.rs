//! Canonical protocol types for Octo agent communication.
//!
//! This crate defines the canonical message, event, and command formats used across
//! all Octo communication boundaries:
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
//! 1. **Messages are persistent, events are ephemeral.** Messages are stored in hstry.
//!    Events drive the UI but are not persisted as conversation content.
//! 2. **Parts are the atomic content unit.** Re-exported from `hstry_core::parts`.
//! 3. **Events form a state machine.** The frontend can derive UI state from any single event.
//! 4. **Agent-agnostic.** Supports any harness. Agent-specific features use `x-*` extensions.

pub mod commands;
pub mod delegation;
pub mod events;
pub mod messages;
pub mod runner;

// Re-export hstry-core part types as the canonical content unit.
pub use hstry_core::parts::{FileRange, MediaSource, Part, Sender, SenderType, ToolStatus};
