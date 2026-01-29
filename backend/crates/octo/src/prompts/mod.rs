//! Security prompt system for runtime access approvals.
//!
//! This module provides infrastructure for octo-guard and octo-ssh-proxy
//! to request user approval for sensitive operations.
//!
//! ## Architecture
//!
//! ```text
//! octo-guard / octo-ssh-proxy
//!     │
//!     ▼
//! PromptManager (in octo server)
//!     │
//!     ├─► WebSocket broadcast to connected UIs
//!     │
//!     └─► Desktop notification (fallback)
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! // Create a prompt request
//! let request = PromptRequest::file_access("~/.kube/config", "read");
//!
//! // Submit and wait for response
//! let response = prompt_manager.request(request).await?;
//!
//! match response.action {
//!     PromptAction::AllowOnce => { /* grant access once */ }
//!     PromptAction::AllowSession => { /* cache approval */ }
//!     PromptAction::Deny => { /* reject access */ }
//! }
//! ```

mod manager;
mod models;
mod routes;

pub use manager::PromptManager;
pub use models::{
    Prompt, PromptAction, PromptMessage, PromptRequest, PromptResponse, PromptSource, PromptStatus,
    PromptType,
};
pub use routes::prompt_routes;
