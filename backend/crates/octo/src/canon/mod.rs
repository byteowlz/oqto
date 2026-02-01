//! Canonical message format for Octo.
//!
//! This module defines the unified message format used across all agent types
//! (Pi, Claude Code, OpenCode, etc.). The format is aligned with hstry's
//! canonical schema to enable seamless history synchronization.
//!
//! ## Design Principles
//!
//! 1. **Two-level content**: `content` (flattened text for search) + `parts` (structured)
//! 2. **Timestamps in milliseconds**: Unix epoch milliseconds for TypeScript compatibility
//! 3. **Role normalization**: Different agent terms map to canonical roles
//! 4. **Extension points**: `x-*` part types for agent-specific content
//!
//! ## Usage
//!
//! ```rust,ignore
//! use octo::canon::{CanonMessage, CanonPart, MessageRole};
//!
//! let msg = CanonMessage {
//!     id: "msg_123".to_string(),
//!     role: MessageRole::Assistant,
//!     content: "Hello! Let me help you.".to_string(),
//!     parts: vec![
//!         CanonPart::text("Hello! Let me help you."),
//!     ],
//!     created_at: 1700000000000,
//!     ..Default::default()
//! };
//! ```

mod from_opencode;
mod from_pi;
mod types;

pub use from_opencode::*;
pub use from_pi::*;
pub use types::*;
