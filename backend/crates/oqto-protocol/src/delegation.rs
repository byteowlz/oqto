//! Cross-agent delegation types.
//!
//! Delegation allows one session to send a message to another session and receive
//! its response. This enables:
//!
//! - **User-initiated**: `@@agent:session` syntax in the chat input
//! - **Agent-initiated**: Via the `delegate` tool or `oqto-delegate` CLI
//! - **Cross-runner**: Backend routes between runners transparently
//!
//! ## Flow
//!
//! ```text
//! Session A                  Backend                   Runner B / Session B
//!     |                         |                           |
//!     |-- delegate command ---->|                           |
//!     |                         |-- prompt command -------->|
//!     |<-- delegate.start ------|                           |
//!     |                         |<-- stream events ---------|
//!     |<-- delegate.delta ------|                           |
//!     |<-- delegate.delta ------|                           |
//!     |                         |<-- stream.done -----------|
//!     |<-- delegate.end --------|                           |
//! ```
//!
//! ## Permission model
//!
//! The backend checks `DelegationPermission` before forwarding. Each delegation
//! can optionally specify a sandbox profile that restricts what the target agent
//! can do while processing the request.
//!
//! ## LLM integration
//!
//! For the target agent, the delegation arrives as a normal user prompt with
//! sender identity inlined: `[pi:ses_abc]: What's the git branch?`
//! The target agent responds normally. The runner wraps the response with
//! sender metadata so the originating session can attribute it correctly.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Sender;
use crate::messages::Message;

// ============================================================================
// Delegation commands
// ============================================================================

/// Request to delegate a message to another session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateRequest {
    /// Target session ID.
    pub target_session_id: String,

    /// Target runner ID (backend resolves if omitted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_runner_id: Option<String>,

    /// The message to send to the target agent.
    pub message: String,

    /// Whether to wait for the response (sync) or return immediately (async).
    pub mode: DelegateMode,

    /// Optional sandbox profile to apply to the target session for this request.
    /// If set, overrides the target session's default sandbox with a more
    /// restrictive profile for the duration of this delegation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_profile: Option<String>,

    /// Maximum time to wait for a response (ms). Applies to both sync and async.
    /// Default: 300000 (5 minutes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,

    /// Maximum output tokens for the delegated response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,

    /// Opaque context passed through to the target (e.g. file paths, instructions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Value>,
}

/// Whether to block for the response or fire-and-forget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegateMode {
    /// Block until the target responds (or timeout). The response is returned
    /// as the tool result / delegate.end event.
    Sync,
    /// Return immediately with a request_id. The response arrives later as
    /// delegate.end, which the runner can inject as a follow-up message.
    Async,
}

/// Cancel an in-flight async delegation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateCancelRequest {
    /// The delegation request ID to cancel.
    pub request_id: String,
}

// ============================================================================
// Delegation events
// ============================================================================

/// Delegation started (emitted to the originating session).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateStarted {
    /// Unique ID for this delegation request.
    pub request_id: String,

    /// Which session is handling the delegation.
    pub target_session_id: String,

    /// Which runner the target is on.
    pub target_runner_id: String,

    /// Whether this is sync or async.
    pub mode: DelegateMode,
}

/// Streaming delta from the delegated agent (text content).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateDelta {
    /// The delegation request ID.
    pub request_id: String,

    /// Text delta from the target agent.
    pub delta: String,
}

/// Delegation completed successfully.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateCompleted {
    /// The delegation request ID.
    pub request_id: String,

    /// The complete response message from the target agent.
    /// Includes sender metadata identifying the target session.
    pub response: Message,

    /// Sender identity of the target agent (for attribution).
    pub responder: Sender,

    /// How long the delegation took (ms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

/// Delegation failed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateError {
    /// The delegation request ID.
    pub request_id: String,

    /// What went wrong.
    pub error: String,

    /// Error category for programmatic handling.
    pub code: DelegateErrorCode,
}

/// Delegation error categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegateErrorCode {
    /// Target session not found or not running.
    TargetNotFound,
    /// Permission denied by delegation policy.
    PermissionDenied,
    /// Target agent timed out.
    Timeout,
    /// Target agent encountered an error while processing.
    TargetError,
    /// Delegation was cancelled.
    Cancelled,
    /// Target runner is unreachable.
    RunnerUnreachable,
}

// ============================================================================
// Runner-local delegation routing
// ============================================================================

/// Routing decision for a delegation request.
///
/// The runner checks whether the target session is local before deciding
/// whether to handle delegation directly or escalate to the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegateRouting {
    /// Target session is managed by this runner -- handle locally.
    Local,
    /// Target session is on another runner -- escalate to backend.
    Escalate,
}

/// Escalation request from runner to backend for cross-runner delegation.
///
/// Sent when the runner receives a delegation command for a session it doesn't
/// manage. The backend routes the request to the correct runner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateEscalation {
    /// Originating session (the one that requested the delegation).
    pub source_session_id: String,

    /// The full delegation request.
    pub request: DelegateRequest,

    /// Correlation ID for matching the response stream back to the source.
    pub correlation_id: String,
}

// ============================================================================
// Delegation permissions
// ============================================================================

/// Permission policy for delegation between sessions.
///
/// Stored in the backend's configuration or per-workspace settings.
/// The backend checks this before forwarding any delegation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationPermission {
    /// Source session pattern (glob or exact ID). "*" matches all.
    pub source: String,

    /// Target session pattern (glob or exact ID). "*" matches all.
    pub target: String,

    /// Whether this rule allows or denies.
    pub effect: PermissionEffect,

    /// Required sandbox profile for the target when this delegation occurs.
    /// If set, the target must use at least this restrictive a profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_sandbox: Option<String>,

    /// Maximum delegation depth (to prevent infinite delegation chains).
    /// Default: 3.
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,

    /// Whether the source can delegate in async mode.
    #[serde(default = "default_true")]
    pub allow_async: bool,

    /// Optional description for this rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn default_max_depth() -> u32 {
    3
}

fn default_true() -> bool {
    true
}

/// Whether a permission rule allows or denies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionEffect {
    Allow,
    Deny,
}

/// Delegation context tracked by the backend for in-flight requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationContext {
    /// Unique request ID.
    pub request_id: String,

    /// The session that initiated the delegation.
    pub source_session_id: String,

    /// The runner the source is on.
    pub source_runner_id: String,

    /// The user who initiated (directly or via agent tool call).
    pub user_id: String,

    /// Target session.
    pub target_session_id: String,

    /// Target runner.
    pub target_runner_id: String,

    /// Sync or async.
    pub mode: DelegateMode,

    /// Sandbox profile applied to the target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_profile: Option<String>,

    /// Current depth in the delegation chain (0 = direct user request).
    pub depth: u32,

    /// When this delegation was created (Unix ms).
    pub created_at: i64,

    /// Timeout deadline (Unix ms).
    pub deadline: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delegate_request_serialization() {
        let req = DelegateRequest {
            target_session_id: "ses_xyz".to_string(),
            target_runner_id: None,
            message: "What's the git branch?".to_string(),
            mode: DelegateMode::Sync,
            sandbox_profile: Some("readonly".to_string()),
            timeout_ms: Some(30000),
            max_tokens: None,
            context: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"mode\":\"sync\""));
        assert!(json.contains("\"sandbox_profile\":\"readonly\""));

        let parsed: DelegateRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mode, DelegateMode::Sync);
        assert_eq!(parsed.sandbox_profile.as_deref(), Some("readonly"));
    }

    #[test]
    fn test_delegate_error_codes() {
        let err = DelegateError {
            request_id: "del-1".to_string(),
            error: "Session ses_xyz not found".to_string(),
            code: DelegateErrorCode::TargetNotFound,
        };

        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"code\":\"target_not_found\""));
    }

    #[test]
    fn test_permission_serialization() {
        let perm = DelegationPermission {
            source: "ses_abc".to_string(),
            target: "*".to_string(),
            effect: PermissionEffect::Allow,
            required_sandbox: Some("readonly".to_string()),
            max_depth: 2,
            allow_async: true,
            description: Some(
                "Allow ses_abc to delegate to any session in readonly mode".to_string(),
            ),
        };

        let json = serde_json::to_string(&perm).unwrap();
        assert!(json.contains("\"effect\":\"allow\""));
        assert!(json.contains("\"max_depth\":2"));
        assert!(json.contains("\"required_sandbox\":\"readonly\""));
    }

    #[test]
    fn test_delegation_context() {
        let ctx = DelegationContext {
            request_id: "del-1".to_string(),
            source_session_id: "ses_abc".to_string(),
            source_runner_id: "local".to_string(),
            user_id: "user-alice".to_string(),
            target_session_id: "ses_xyz".to_string(),
            target_runner_id: "remote-1".to_string(),
            mode: DelegateMode::Async,
            sandbox_profile: Some("readonly".to_string()),
            depth: 0,
            created_at: 1738764000000,
            deadline: 1738764300000,
        };

        let json = serde_json::to_string(&ctx).unwrap();
        assert!(json.contains("\"depth\":0"));
        assert!(json.contains("\"mode\":\"async\""));
    }

    #[test]
    fn test_delegate_routing_variants() {
        assert_eq!(DelegateRouting::Local, DelegateRouting::Local);
        assert_ne!(DelegateRouting::Local, DelegateRouting::Escalate);
    }

    #[test]
    fn test_delegate_escalation_serialization() {
        let esc = DelegateEscalation {
            source_session_id: "ses_abc".to_string(),
            request: DelegateRequest {
                target_session_id: "ses_xyz".to_string(),
                target_runner_id: Some("remote-1".to_string()),
                message: "What branch are we on?".to_string(),
                mode: DelegateMode::Sync,
                sandbox_profile: None,
                timeout_ms: Some(30000),
                max_tokens: None,
                context: None,
            },
            correlation_id: "corr-1".to_string(),
        };

        let json = serde_json::to_string(&esc).unwrap();
        assert!(json.contains("\"source_session_id\":\"ses_abc\""));
        assert!(json.contains("\"correlation_id\":\"corr-1\""));
        assert!(json.contains("\"target_session_id\":\"ses_xyz\""));

        let parsed: DelegateEscalation = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.source_session_id, "ses_abc");
        assert_eq!(parsed.correlation_id, "corr-1");
        assert_eq!(parsed.request.mode, DelegateMode::Sync);
    }
}
