//! Pure request/response state machine. One outstanding request at a time, with
//! fences on version, correlation, binding, catalog generation and queue cursor.

use crate::catalog::CatalogState;
use crate::contract::{
    RuntimeBinding, RuntimeCapability, RuntimeCommandDisposition, RuntimeCursor, RuntimeError,
    RuntimeEvent, RuntimeEventPayload, RuntimeFamily, RuntimeId, RuntimeOperation,
    RuntimePermissionDecision, RuntimePermissionOutcome, RuntimePermissionSnapshot,
    RuntimeQueueAction, RuntimeQueueActionKind, RuntimeQueueSnapshot, RuntimeRejectCode,
    RuntimeRequest, RuntimeResponse, RuntimeResult, RuntimeRevision, RuntimeUnavailableReason,
    RuntimeVersion,
};
use crate::error::ClientError;
use crate::permissions::PermissionState;
use crate::queue::QueueState;
use crate::receipts::{ReceiptEntry, ReceiptLog, RecordedReceipt};

/// Monotonic, contract-shaped request identities. Correlation only - not authority.
#[derive(Debug, Clone)]
pub struct RequestIdSource {
    prefix: String,
    next: u64,
}

impl RequestIdSource {
    #[must_use]
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            next: 0,
        }
    }

    /// Mint the next request identity.
    pub fn mint(&mut self) -> Result<RuntimeId, ClientError> {
        self.next = self.next.saturating_add(1);
        RuntimeId::try_from(format!("{}-{}", self.prefix, self.next))
            .map_err(|_| ClientError::Decode)
    }
}

impl Default for RequestIdSource {
    fn default() -> Self {
        Self::new("req")
    }
}

/// What a response contributed to consumer state.
#[derive(Debug, Clone, PartialEq)]
pub enum Applied {
    Capabilities(Vec<RuntimeCapability>),
    Catalog,
    CommandReceipt(RuntimeCommandDisposition),
    PermissionSnapshot,
    PermissionReceipt(RuntimePermissionOutcome),
    Policy,
    Queue,
    Errors,
    Unavailable {
        family: RuntimeFamily,
        reason: RuntimeUnavailableReason,
    },
    Rejected(RuntimeRejectCode),
}

/// Session-bound consumer state over one runtime interaction stream.
#[derive(Debug)]
pub struct InteractionSession {
    binding: Option<RuntimeBinding>,
    outstanding: Option<(RuntimeId, RuntimeOperation)>,
    ids: RequestIdSource,
    catalog: CatalogState,
    queue: QueueState,
    permissions: PermissionState,
    receipts: ReceiptLog,
    errors: Vec<RuntimeError>,
    errors_cursor: Option<RuntimeCursor>,
}

impl Default for InteractionSession {
    fn default() -> Self {
        Self::new()
    }
}

impl InteractionSession {
    #[must_use]
    pub fn new() -> Self {
        Self {
            binding: None,
            outstanding: None,
            ids: RequestIdSource::default(),
            catalog: CatalogState::default(),
            queue: QueueState::default(),
            permissions: PermissionState::default(),
            receipts: ReceiptLog::default(),
            errors: Vec::new(),
            errors_cursor: None,
        }
    }

    #[must_use]
    pub fn binding(&self) -> Option<&RuntimeBinding> {
        self.binding.as_ref()
    }

    /// Adopt the host-supplied binding before the first request. A later change
    /// is refused: re-admission belongs to the transport, not to this state.
    pub fn adopt_binding(&mut self, binding: RuntimeBinding) -> Result<(), ClientError> {
        match &self.binding {
            None => {
                self.binding = Some(binding);
                Ok(())
            }
            Some(current) if current == &binding => Ok(()),
            Some(_) => Err(ClientError::BindingChanged),
        }
    }

    #[must_use]
    pub fn catalog(&self) -> &CatalogState {
        &self.catalog
    }

    #[must_use]
    pub fn queue(&self) -> &QueueState {
        &self.queue
    }

    #[must_use]
    pub fn permissions(&self) -> &PermissionState {
        &self.permissions
    }

    #[must_use]
    pub fn receipts(&self) -> &ReceiptLog {
        &self.receipts
    }

    #[must_use]
    pub fn errors(&self) -> &[RuntimeError] {
        &self.errors
    }

    #[must_use]
    pub fn has_outstanding(&self) -> bool {
        self.outstanding.is_some()
    }

    /// Begin a request, refusing to overlap a second one on the same stream.
    fn begin(&mut self, operation: RuntimeOperation) -> Result<RuntimeRequest, ClientError> {
        if self.outstanding.is_some() {
            return Err(ClientError::RequestOutstanding);
        }
        let request_id = self.ids.mint()?;
        self.outstanding = Some((request_id.clone(), operation.clone()));
        Ok(RuntimeRequest {
            version: RuntimeVersion::V1,
            request_id,
            operation,
        })
    }

    pub fn capabilities_request(&mut self) -> Result<RuntimeRequest, ClientError> {
        self.begin(RuntimeOperation::Capabilities)
    }

    pub fn catalog_request(&mut self) -> Result<RuntimeRequest, ClientError> {
        self.begin(RuntimeOperation::CommandCatalog)
    }

    pub fn queue_snapshot_request(&mut self) -> Result<RuntimeRequest, ClientError> {
        self.begin(RuntimeOperation::QueueSnapshot)
    }

    pub fn permission_snapshot_request(&mut self) -> Result<RuntimeRequest, ClientError> {
        self.begin(RuntimeOperation::PermissionSnapshot)
    }

    pub fn error_list_request(
        &mut self,
        after: Option<RuntimeId>,
    ) -> Result<RuntimeRequest, ClientError> {
        self.begin(RuntimeOperation::ErrorList {
            after_event_id: after,
        })
    }

    pub fn receipt_request(
        &mut self,
        operation_id: RuntimeId,
    ) -> Result<RuntimeRequest, ClientError> {
        self.begin(RuntimeOperation::ReceiptGet { operation_id })
    }

    /// Build a dispatch for a command in the current catalog.
    ///
    /// Arguments are preserved byte-for-byte; the command identity comes from the
    /// catalog, never from splitting caller text.
    pub fn dispatch_request(
        &mut self,
        command_id: &str,
        args: &str,
    ) -> Result<RuntimeRequest, ClientError> {
        let command_id =
            RuntimeId::try_from(command_id.to_owned()).map_err(|_| ClientError::UnknownCommand)?;
        let generation = self
            .catalog
            .generation()
            .cloned()
            .ok_or(ClientError::CatalogUnknown)?;
        let command = self
            .catalog
            .find(&command_id)
            .ok_or(ClientError::UnknownCommand)?;
        match &command.availability {
            crate::contract::RuntimeAvailability::Available => {}
            crate::contract::RuntimeAvailability::Unavailable { reason } => {
                return Err(ClientError::CommandUnavailable(*reason));
            }
        }
        self.begin(RuntimeOperation::CommandDispatch {
            catalog_generation: generation,
            command_id,
            args: args.to_owned(),
        })
    }

    /// Build an explicit decision for an active, unexpired request.
    ///
    /// The producer's deadline is authoritative; `now_ms` is the caller's clock.
    pub fn decide_request(
        &mut self,
        handle_id: &str,
        revision: u64,
        decision: RuntimePermissionDecision,
        option_id: Option<&str>,
        now_ms: u64,
    ) -> Result<RuntimeRequest, ClientError> {
        let handle =
            RuntimeId::try_from(handle_id.to_owned()).map_err(|_| ClientError::NotActive)?;
        let revision = RuntimeRevision::try_from(revision).map_err(|_| ClientError::NotActive)?;
        let presentation = self
            .permissions
            .find(&handle, revision)
            .ok_or(ClientError::NotActive)?
            .clone();
        if PermissionState::is_expired(&presentation, now_ms) {
            return Err(ClientError::Expired);
        }
        let option = match option_id {
            Some(option) => {
                let option = RuntimeId::try_from(option.to_owned())
                    .map_err(|_| ClientError::UnknownOption)?;
                if !PermissionState::issued_option(&presentation, &option) {
                    return Err(ClientError::UnknownOption);
                }
                Some(option)
            }
            None => None,
        };
        let binding = self
            .binding
            .as_ref()
            .ok_or(ClientError::MissingController)?;
        let controller_id = binding
            .controller_id
            .clone()
            .ok_or(ClientError::MissingController)?;
        let controller_generation = binding
            .controller_generation
            .clone()
            .ok_or(ClientError::MissingController)?;
        self.begin(RuntimeOperation::PermissionDecide {
            producer_generation: binding.runtime_generation.clone(),
            handle_id: handle,
            request_revision: revision,
            controller_id,
            controller_generation,
            decision,
            option_id: option,
        })
    }

    /// Build a queue mutation only when the source declared it and a cursor is known.
    pub fn queue_action_request(
        &mut self,
        action: RuntimeQueueAction,
    ) -> Result<RuntimeRequest, ClientError> {
        let expected = self
            .queue
            .cursor()
            .cloned()
            .ok_or(ClientError::UnknownQueueCursor)?;
        if !self.queue.supports(action_kind(&action)) {
            return Err(ClientError::UnsupportedAction);
        }
        self.begin(RuntimeOperation::QueueApply { expected, action })
    }

    /// Apply a producer response, refusing anything uncorrelated or stale.
    pub fn apply(&mut self, response: RuntimeResponse) -> Result<Applied, ClientError> {
        if response.version != RuntimeVersion::V1 {
            return Err(ClientError::VersionMismatch);
        }
        let outstanding = self
            .outstanding
            .take()
            .ok_or(ClientError::NoOutstandingRequest)?;
        let (request_id, operation) = outstanding.clone();
        if response.request_id != request_id {
            self.outstanding = Some(outstanding);
            return Err(ClientError::UncorrelatedResponse);
        }
        if !response.result.matches_operation(&operation) {
            self.outstanding = Some(outstanding);
            return Err(ClientError::ResultMismatch);
        }
        self.check_binding(&response.binding)?;
        let _ = operation;
        self.fold(response.result, request_id)
    }

    fn check_binding(&mut self, binding: &RuntimeBinding) -> Result<(), ClientError> {
        match &self.binding {
            None => {
                self.binding = Some(binding.clone());
                Ok(())
            }
            Some(current) if current == binding => Ok(()),
            Some(_) => Err(ClientError::BindingChanged),
        }
    }

    fn runtime_generation(&self) -> Result<crate::contract::RuntimeGeneration, ClientError> {
        self.binding
            .as_ref()
            .map(|binding| binding.runtime_generation.clone())
            .ok_or(ClientError::BindingChanged)
    }

    fn fold(
        &mut self,
        result: RuntimeResult,
        request_id: RuntimeId,
    ) -> Result<Applied, ClientError> {
        match result {
            RuntimeResult::Capabilities { capabilities } => Ok(Applied::Capabilities(capabilities)),
            RuntimeResult::Catalog { catalog } => {
                self.catalog.apply(catalog);
                Ok(Applied::Catalog)
            }
            RuntimeResult::CommandReceipt { receipt } => {
                let operation_id = Some(receipt.operation_id.clone());
                let disposition = receipt.disposition;
                self.receipts.record(ReceiptEntry {
                    request_id,
                    operation_id,
                    receipt: RecordedReceipt::Command {
                        disposition,
                        rejection: receipt.rejection,
                        effect: None,
                        error_id: None,
                    },
                });
                Ok(Applied::CommandReceipt(disposition))
            }
            RuntimeResult::Permissions { snapshot } => {
                self.apply_permissions(snapshot);
                Ok(Applied::PermissionSnapshot)
            }
            RuntimeResult::PermissionReceipt { receipt } => {
                let outcome = receipt.outcome;
                self.receipts.record(ReceiptEntry {
                    request_id,
                    operation_id: None,
                    receipt: RecordedReceipt::Permission { outcome },
                });
                Ok(Applied::PermissionReceipt(outcome))
            }
            RuntimeResult::Policy { .. } => Ok(Applied::Policy),
            RuntimeResult::Queue { snapshot } => {
                self.apply_queue(snapshot)?;
                Ok(Applied::Queue)
            }
            RuntimeResult::Errors { snapshot } => {
                self.absorb_errors(&snapshot.cursor, snapshot.errors);
                Ok(Applied::Errors)
            }
            RuntimeResult::Unavailable { family, reason } => {
                self.receipts.record(ReceiptEntry {
                    request_id,
                    operation_id: None,
                    receipt: RecordedReceipt::Unavailable { family, reason },
                });
                Ok(Applied::Unavailable { family, reason })
            }
            RuntimeResult::Rejected { code } => {
                self.receipts.record(ReceiptEntry {
                    request_id,
                    operation_id: None,
                    receipt: RecordedReceipt::Rejected { code },
                });
                Ok(Applied::Rejected(code))
            }
        }
    }

    fn apply_permissions(&mut self, snapshot: RuntimePermissionSnapshot) {
        self.permissions.apply_snapshot(snapshot);
    }

    fn apply_queue(&mut self, snapshot: RuntimeQueueSnapshot) -> Result<(), ClientError> {
        let generation = self.runtime_generation()?;
        self.queue.apply_snapshot(snapshot, &generation)
    }

    fn absorb_errors(&mut self, cursor: &RuntimeCursor, errors: Vec<RuntimeError>) {
        for error in errors {
            if !self
                .errors
                .iter()
                .any(|known| known.event_id == error.event_id)
            {
                self.errors.push(error);
            }
        }
        self.errors_cursor = Some(cursor.clone());
    }

    #[must_use]
    pub fn errors_cursor(&self) -> Option<&RuntimeCursor> {
        self.errors_cursor.as_ref()
    }

    /// Apply a pushed event. Events extend state; they never authorize work.
    pub fn apply_event(&mut self, event: RuntimeEvent) -> Result<(), ClientError> {
        if event.version != RuntimeVersion::V1 {
            return Err(ClientError::VersionMismatch);
        }
        if let Some(binding) = &self.binding {
            if binding != &event.binding {
                return Err(ClientError::BindingChanged);
            }
        } else {
            self.binding = Some(event.binding.clone());
        }
        match event.payload {
            RuntimeEventPayload::Queue { snapshot } => self.apply_queue(snapshot),
            RuntimeEventPayload::Permissions { snapshot } => {
                self.apply_permissions(snapshot);
                Ok(())
            }
            RuntimeEventPayload::Error { error } => {
                if !self
                    .errors
                    .iter()
                    .any(|known| known.event_id == error.event_id)
                {
                    self.errors.push(error);
                }
                Ok(())
            }
            RuntimeEventPayload::CommandOutcome {
                operation_id,
                state,
                error_id,
            } => {
                self.receipts.apply_outcome(&operation_id, state, error_id);
                Ok(())
            }
        }
    }
}

/// Action kind for capability checks; the action type is owned by the contract crate.
#[must_use]
fn action_kind(action: &RuntimeQueueAction) -> RuntimeQueueActionKind {
    match action {
        RuntimeQueueAction::Steer { .. } => RuntimeQueueActionKind::Steer,
        RuntimeQueueAction::FollowUp { .. } => RuntimeQueueActionKind::FollowUp,
        RuntimeQueueAction::CancelItem { .. } => RuntimeQueueActionKind::CancelItem,
        RuntimeQueueAction::Clear => RuntimeQueueActionKind::Clear,
        RuntimeQueueAction::UpdateItem { .. } => RuntimeQueueActionKind::UpdateItem,
        RuntimeQueueAction::Reorder { .. } => RuntimeQueueActionKind::Reorder,
    }
}
