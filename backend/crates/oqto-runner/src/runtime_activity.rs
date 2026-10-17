//! Bounded retained input state for read-only activity snapshots. Never opens a harness.
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use oqto_protocol::events::{EventPayload, InputRequest};
use serde::{Deserialize, Serialize};

pub const ACTIVITY_VERSION: u32 = 1;
pub const MAX_ACTIVITY_SESSIONS: usize = 512;
const MAX_INPUTS: usize = 128;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionActivity {
    pub session_id: String,
    pub external_history_id: Option<String>,
    pub workspace_path: std::path::PathBuf,
    pub execution_id: String,
    pub revision: u64,
    pub running: bool,
    pub attention_count: u32,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySnapshot {
    pub version: u32,
    pub complete: bool,
    pub sessions: Vec<SessionActivity>,
}

/// No titles, messages, credentials or prompt content are retained here.
pub(crate) struct PendingInputs {
    pub execution_id: String,
    revision: u64,
    pending: BTreeMap<String, Option<Instant>>,
    complete: bool,
}

impl Default for PendingInputs {
    fn default() -> Self {
        Self::new()
    }
}

impl PendingInputs {
    pub fn new() -> Self {
        Self {
            execution_id: uuid::Uuid::new_v4().to_string(),
            revision: 0,
            pending: BTreeMap::new(),
            complete: true,
        }
    }

    pub fn observe(&mut self, event: &EventPayload, now: Instant) {
        match event {
            EventPayload::AgentInputNeeded { request } => {
                let (id, timeout) = match request {
                    InputRequest::Select {
                        request_id,
                        timeout,
                        ..
                    }
                    | InputRequest::Confirm {
                        request_id,
                        timeout,
                        ..
                    }
                    | InputRequest::Input {
                        request_id,
                        timeout,
                        ..
                    } => (request_id, *timeout),
                    InputRequest::Permission { request_id, .. } => (request_id, None),
                };
                self.pending
                    .retain(|_, deadline| deadline.is_none_or(|end| end > now));
                let expires = timeout.map(|ms| now.checked_add(Duration::from_millis(ms)));
                if id.is_empty()
                    || id.len() > 256
                    || matches!(expires, Some(None))
                    || (self.pending.len() >= MAX_INPUTS && !self.pending.contains_key(id))
                {
                    self.complete = false;
                } else {
                    self.pending.insert(id.clone(), expires.flatten());
                }
                self.revision = self.revision.saturating_add(1);
            }
            EventPayload::AgentInputResolved { request_id } => self.resolved(request_id),
            EventPayload::AgentIdle { .. }
            | EventPayload::SessionCreated { .. }
            | EventPayload::SessionClosed { .. }
            | EventPayload::AgentError {
                recoverable: false, ..
            } => self.clear(),
            _ => {}
        }
    }

    /// A logical replacement must never carry pending requests from the old execution.
    pub fn replace_execution(&mut self) {
        self.execution_id = uuid::Uuid::new_v4().to_string();
        self.pending.clear();
        self.revision = 0;
        self.complete = false;
    }

    pub fn resolved(&mut self, id: &str) {
        if self.pending.remove(id).is_some() {
            self.revision = self.revision.saturating_add(1);
        }
    }

    pub fn clear(&mut self) {
        self.pending.clear();
        self.complete = true;
        self.revision = self.revision.saturating_add(1);
    }

    pub fn counts(&self, now: Instant) -> (u64, u32, bool) {
        let count = self
            .pending
            .values()
            .filter(|deadline| deadline.is_none_or(|end| end > now))
            .count();
        (self.revision, count as u32, self.complete)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(id: &str, timeout: Option<u64>) -> EventPayload {
        EventPayload::AgentInputNeeded {
            request: InputRequest::Confirm {
                request_id: id.into(),
                title: "Synthetic approval".into(),
                message: "Synthetic question".into(),
                timeout,
            },
        }
    }
    #[test]
    fn replacement_has_new_epoch_and_cannot_keep_old_attention_or_claim_idle() {
        let now = Instant::now();
        let mut inputs = PendingInputs::new();
        inputs.observe(&input("old", None), now);
        let epoch = inputs.execution_id.clone();
        inputs.replace_execution();
        assert_ne!(inputs.execution_id, epoch);
        let (_, count, complete) = inputs.counts(now);
        assert_eq!(count, 0);
        assert!(!complete);
        inputs.observe(
            &EventPayload::AgentIdle {
                message_version: None,
            },
            now,
        );
        assert!(inputs.counts(now).2);
    }

    #[test]
    fn retained_deduplicated_bounded_and_no_secret_content() {
        let mut state = PendingInputs::new();
        let now = Instant::now();
        state.observe(&input("a", None), now);
        state.observe(&input("a", None), now);
        assert_eq!(state.counts(now).1, 1);
        for n in 0..MAX_INPUTS {
            state.observe(&input(&format!("q{n}"), None), now);
        }
        assert_eq!(state.counts(now).1, MAX_INPUTS as u32);
        assert!(!state.counts(now).2);
        state.clear();
        assert_eq!(state.counts(now).1, 0);
        assert!(state.counts(now).2);
    }
    #[test]
    fn timeout_resolution_idle_and_restart_are_explicit() {
        let mut state = PendingInputs::new();
        let now = Instant::now();
        state.observe(&input("a", Some(10)), now);
        assert_eq!(state.counts(now).1, 1);
        assert_eq!(state.counts(now + Duration::from_millis(11)).1, 0);
        state.observe(&input("b", None), now);
        state.resolved("b");
        assert_eq!(state.counts(now).1, 1);
        state.observe(
            &EventPayload::AgentIdle {
                message_version: None,
            },
            now,
        );
        assert_eq!(state.counts(now).1, 0);
        assert_ne!(state.execution_id, PendingInputs::new().execution_id);
    }
    #[test]
    fn working_does_not_clear_outstanding_attention_and_unknown_events_do_not_mutate() {
        let mut state = PendingInputs::new();
        let now = Instant::now();
        state.observe(&input("a", None), now);
        let old = state.counts(now);
        state.observe(
            &EventPayload::AgentWorking {
                phase: oqto_protocol::events::AgentPhase::Generating,
                detail: None,
            },
            now,
        );
        assert_eq!(state.counts(now), old);
        state.observe(
            &EventPayload::AgentInputResolved {
                request_id: "a".into(),
            },
            now,
        );
        assert_eq!(state.counts(now).1, 0);
    }
}
