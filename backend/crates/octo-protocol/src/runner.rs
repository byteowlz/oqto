//! Runner wire protocol types.
//!
//! These types define the JSON-RPC messages exchanged between the backend and
//! runner daemons over persistent connections (Unix socket for local, WebSocket
//! for remote).
//!
//! The runner is responsible for:
//! - Spawning agent processes (pi, opencode, etc.)
//! - Translating native agent events into canonical events
//! - Monitoring process health via PID
//! - Forwarding canonical commands to agent stdin

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::commands::CommandPayload;
use crate::delegation::DelegateEscalation;
use crate::events::{AgentPhase, EventPayload, ProcessHealth};

// ============================================================================
// Runner registration
// ============================================================================

/// Runner hello message (sent by runner on connect).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerHello {
    /// Unique runner identifier.
    pub runner_id: String,

    /// Hostname of the machine running the runner.
    pub hostname: String,

    /// Which agent harnesses this runner can spawn.
    pub harnesses: Vec<String>,

    /// Maximum concurrent sessions.
    pub max_sessions: u32,

    /// Runner version.
    pub version: String,

    /// Operating system.
    pub os: String,
}

/// Backend acknowledgment to runner hello.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerWelcome {
    /// Echoed runner ID (confirming registration).
    pub runner_id: String,
}

// ============================================================================
// Wire messages (newline-delimited JSON)
// ============================================================================

/// Message from backend to runner.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackendToRunner {
    /// Forward a canonical command to a session.
    Command {
        session_id: String,
        user_id: String,
        #[serde(flatten)]
        cmd: Box<CommandPayload>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },

    /// Acknowledge runner registration.
    Welcome(RunnerWelcome),
}

/// Message from runner to backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunnerToBackend {
    /// Runner registration.
    #[serde(rename = "runner.hello")]
    Hello(RunnerHello),

    /// Canonical event from a session.
    Event {
        session_id: String,
        #[serde(flatten)]
        event: Box<EventPayload>,
        ts: i64,
    },

    /// Response to a command.
    Response {
        id: String,
        cmd: String,
        success: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    /// Periodic heartbeat with per-session process telemetry.
    #[serde(rename = "runner.heartbeat")]
    Heartbeat {
        runner_id: String,
        uptime_s: u64,
        sessions: Vec<SessionTelemetry>,
    },

    /// Escalate a delegation to the backend for cross-runner routing.
    ///
    /// Sent when the runner receives a delegation command targeting a session
    /// it doesn't manage. The backend routes the request to the correct runner
    /// and streams events back.
    #[serde(rename = "delegate.escalate")]
    DelegateEscalate(DelegateEscalation),
}

/// Per-session telemetry in runner heartbeat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTelemetry {
    pub session_id: String,
    pub harness: String,
    pub process: ProcessHealth,
}

// ============================================================================
// Runner session state machine
// ============================================================================

/// Per-session state maintained by the runner's event translator.
///
/// The runner tracks this state to correctly translate native agent events
/// into canonical events. The key rule: `agent_start` and `agent_end` are
/// authoritative idle/working transitions. Extension `setStatus` events
/// only refine the phase WITHIN the working state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SessionState {
    /// Session process is starting up.
    #[default]
    Initializing,
    /// Agent is idle, waiting for input.
    Idle,
    /// Agent is working with a specific phase.
    Working {
        phase: AgentPhase,
        detail: Option<String>,
    },
    /// Session process has exited/crashed.
    Dead,
}

impl SessionState {
    /// Apply a state transition from a native agent lifecycle event.
    ///
    /// Returns the new state and the canonical event to emit (if any).
    pub fn on_agent_start(&mut self) -> EventPayload {
        *self = Self::Working {
            phase: AgentPhase::Generating,
            detail: None,
        };
        EventPayload::AgentWorking {
            phase: AgentPhase::Generating,
            detail: None,
        }
    }

    /// Transition to idle on agent_end.
    pub fn on_agent_end(&mut self) -> EventPayload {
        *self = Self::Idle;
        EventPayload::AgentIdle
    }

    /// Refine the phase within the working state (from extension setStatus).
    ///
    /// If not currently working, ignores the update (does not transition to working).
    /// If the status is cleared (None), falls back to Generating.
    pub fn on_extension_phase(
        &mut self,
        phase: Option<AgentPhase>,
        detail: Option<String>,
    ) -> Option<EventPayload> {
        if !matches!(self, Self::Working { .. }) {
            return None;
        }

        let (new_phase, new_detail) = match phase {
            Some(p) => (p, detail),
            None => (AgentPhase::Generating, None),
        };

        *self = Self::Working {
            phase: new_phase,
            detail: new_detail.clone(),
        };

        Some(EventPayload::AgentWorking {
            phase: new_phase,
            detail: new_detail,
        })
    }

    /// Transition to specific working phase from native events
    /// (e.g. tool_execution_start, auto_compaction_start).
    pub fn on_native_phase(&mut self, phase: AgentPhase, detail: Option<String>) -> EventPayload {
        *self = Self::Working {
            phase,
            detail: detail.clone(),
        };
        EventPayload::AgentWorking { phase, detail }
    }

    /// Mark the session as dead (process exited).
    pub fn on_process_exit(&mut self, error: String) -> EventPayload {
        *self = Self::Dead;
        EventPayload::AgentError {
            error,
            recoverable: false,
            phase: None,
        }
    }

    /// Whether the session is currently in a working state.
    pub fn is_working(&self) -> bool {
        matches!(self, Self::Working { .. })
    }

    /// Whether the session is idle.
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }
}

// ============================================================================
// Mux channel
// ============================================================================

/// WebSocket multiplexer channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Canonical agent protocol (replaces "pi" and "session").
    Agent,
    /// File operations.
    Files,
    /// Terminal I/O.
    Terminal,
    /// History queries.
    Hstry,
    /// Issue tracking.
    Trx,
    /// Connection lifecycle.
    System,
}

/// Additional environment to pass to the agent process.
pub type AgentEnv = HashMap<String, String>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_state_machine() {
        let mut state = SessionState::default();
        assert_eq!(state, SessionState::Initializing);

        // agent_start -> working(generating)
        let event = state.on_agent_start();
        assert!(matches!(
            event,
            EventPayload::AgentWorking {
                phase: AgentPhase::Generating,
                ..
            }
        ));
        assert!(state.is_working());

        // extension: tool_running:bash
        let event = state.on_extension_phase(Some(AgentPhase::ToolRunning), Some("bash".into()));
        assert!(event.is_some());
        assert!(matches!(
            state,
            SessionState::Working {
                phase: AgentPhase::ToolRunning,
                ..
            }
        ));

        // extension: clear -> fall back to generating
        let event = state.on_extension_phase(None, None);
        assert!(event.is_some());
        assert!(matches!(
            state,
            SessionState::Working {
                phase: AgentPhase::Generating,
                ..
            }
        ));

        // agent_end -> idle
        let event = state.on_agent_end();
        assert!(matches!(event, EventPayload::AgentIdle));
        assert!(state.is_idle());

        // extension while idle -> ignored
        let event = state.on_extension_phase(Some(AgentPhase::ToolRunning), None);
        assert!(event.is_none());
        assert!(state.is_idle());
    }

    #[test]
    fn test_session_state_process_exit() {
        let mut state = SessionState::Idle;
        let event = state.on_process_exit("Process exited with code 1".to_string());
        assert!(matches!(
            event,
            EventPayload::AgentError {
                recoverable: false,
                ..
            }
        ));
        assert_eq!(state, SessionState::Dead);
    }

    #[test]
    fn test_runner_hello_serialization() {
        let hello = RunnerHello {
            runner_id: "wkst-alice-01".to_string(),
            hostname: "alice-workstation".to_string(),
            harnesses: vec!["pi".to_string(), "opencode".to_string()],
            max_sessions: 10,
            version: "0.1.0".to_string(),
            os: "linux".to_string(),
        };

        let json = serde_json::to_string(&hello).unwrap();
        assert!(json.contains("\"runner_id\":\"wkst-alice-01\""));
        assert!(json.contains("\"harnesses\":[\"pi\",\"opencode\"]"));
    }

    #[test]
    fn test_runner_to_backend_event() {
        let msg = RunnerToBackend::Event {
            session_id: "ses_abc".to_string(),
            event: Box::new(EventPayload::AgentWorking {
                phase: AgentPhase::Generating,
                detail: None,
            }),
            ts: 1738764000000,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"event\""));
        assert!(json.contains("\"session_id\":\"ses_abc\""));
    }

    #[test]
    fn test_runner_to_backend_delegate_escalate() {
        use crate::delegation::{DelegateMode, DelegateRequest};

        let msg = RunnerToBackend::DelegateEscalate(DelegateEscalation {
            source_session_id: "ses_abc".to_string(),
            request: DelegateRequest {
                target_session_id: "ses_xyz".to_string(),
                target_runner_id: Some("remote-1".to_string()),
                message: "What branch?".to_string(),
                mode: DelegateMode::Sync,
                sandbox_profile: None,
                timeout_ms: None,
                max_tokens: None,
                context: None,
            },
            correlation_id: "corr-42".to_string(),
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"delegate.escalate\""));
        assert!(json.contains("\"source_session_id\":\"ses_abc\""));
        assert!(json.contains("\"correlation_id\":\"corr-42\""));
    }
}
