//! Authoritative queue view. Unknown is not empty: absence of facts is a state,
//! not zero pending prompts, and a cursor is never invented.

use crate::ClientError;
use crate::contract::{
    RuntimeCursor, RuntimeGeneration, RuntimeQueueActionKind, RuntimeQueueCompleteness,
    RuntimeQueueItem, RuntimeQueueSnapshot,
};

/// Queue facts exactly as the owning engine reported them.
#[derive(Debug, Default, Clone)]
pub struct QueueState {
    completeness: Option<RuntimeQueueCompleteness>,
    cursor: Option<RuntimeCursor>,
    steering: Vec<RuntimeQueueItem>,
    follow_up: Vec<RuntimeQueueItem>,
    supported: Vec<RuntimeQueueActionKind>,
}

impl QueueState {
    /// Whether any snapshot has been accepted for this session.
    #[must_use]
    pub fn is_known(&self) -> bool {
        self.completeness.is_some()
    }

    #[must_use]
    pub fn completeness(&self) -> Option<RuntimeQueueCompleteness> {
        self.completeness
    }

    /// True only with current, complete evidence and no queued prompts.
    /// Unknown, partial or unavailable state is never "nothing queued".
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.completeness == Some(RuntimeQueueCompleteness::Complete)
            && self.steering.is_empty()
            && self.follow_up.is_empty()
    }

    /// True when the source cannot show the whole queue, including zero-row cases.
    #[must_use]
    pub fn is_unknown(&self) -> bool {
        matches!(
            self.completeness,
            None | Some(RuntimeQueueCompleteness::Unknown)
        )
    }

    #[must_use]
    pub fn cursor(&self) -> Option<&RuntimeCursor> {
        self.cursor.as_ref()
    }

    #[must_use]
    pub fn steering(&self) -> &[RuntimeQueueItem] {
        &self.steering
    }

    #[must_use]
    pub fn follow_up(&self) -> &[RuntimeQueueItem] {
        &self.follow_up
    }

    /// Count of visible items; not proof that the queue is complete.
    #[must_use]
    pub fn known_len(&self) -> usize {
        self.steering.len() + self.follow_up.len()
    }

    #[must_use]
    pub fn supports(&self, kind: RuntimeQueueActionKind) -> bool {
        self.supported.contains(&kind)
    }

    #[must_use]
    pub fn supported_actions(&self) -> &[RuntimeQueueActionKind] {
        &self.supported
    }

    /// Accept a source snapshot, refusing a foreign runtime generation and a
    /// revision that goes backwards within the same generation.
    pub fn apply_snapshot(
        &mut self,
        snapshot: RuntimeQueueSnapshot,
        bound_generation: &RuntimeGeneration,
    ) -> Result<(), ClientError> {
        if &snapshot.cursor.generation != bound_generation {
            return Err(ClientError::ForeignQueueGeneration);
        }
        if let Some(current) = &self.cursor
            && current.generation == snapshot.cursor.generation
            && u64::from(snapshot.cursor.revision) < u64::from(current.revision)
        {
            return Err(ClientError::StaleQueueRevision);
        }
        self.completeness = Some(snapshot.completeness);
        self.cursor = Some(snapshot.cursor);
        self.steering = snapshot.steering;
        self.follow_up = snapshot.follow_up;
        self.supported = snapshot.supported_actions;
        Ok(())
    }
}
