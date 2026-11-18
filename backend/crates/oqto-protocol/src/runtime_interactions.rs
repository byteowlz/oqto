//! Canonical extended-runtime DTOs, not authority or active transport routing.
//! Producer/engine state is authoritative; client echoes and schema validity grant nothing.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const MAX_RUNTIME_FRAME_BYTES: usize = 256 * 1024;
/// Explicit source-result support; not engine availability or authenticated host admission.
pub const RUNTIME_SOURCE_CAPABILITY: &str = "runtime_interactions_source_v1";
pub const MAX_SAFE_REVISION: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(try_from = "u8", into = "u8")]
pub struct RuntimeVersion(
    #[schemars(range(min = 1, max = 1))]
    #[ts(type = "1")]
    u8,
);
impl RuntimeVersion {
    pub const V1: Self = Self(1);
}
impl From<RuntimeVersion> for u8 {
    fn from(value: RuntimeVersion) -> Self {
        value.0
    }
}
impl TryFrom<u8> for RuntimeVersion {
    type Error = &'static str;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value == 1 {
            Ok(Self::V1)
        } else {
            Err("unsupported runtime interaction version")
        }
    }
}

/// JSON-safe revision, independent from a request revision or generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(try_from = "u64", into = "u64")]
pub struct RuntimeRevision(
    #[schemars(range(max = 9_007_199_254_740_991_u64))]
    #[ts(type = "number")]
    u64,
);
impl From<RuntimeRevision> for u64 {
    fn from(value: RuntimeRevision) -> Self {
        value.0
    }
}
impl TryFrom<u64> for RuntimeRevision {
    type Error = &'static str;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value <= MAX_SAFE_REVISION {
            Ok(Self(value))
        } else {
            Err("unsafe runtime revision")
        }
    }
}

/// Opaque source-issued identity. Names, timestamps and visible order are not IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(try_from = "String", into = "String")]
pub struct RuntimeId(
    #[schemars(length(min = 1, max = 128), regex(pattern = "^[A-Za-z0-9._:-]+$"))] String,
);
impl From<RuntimeId> for String {
    fn from(value: RuntimeId) -> Self {
        value.0
    }
}
impl TryFrom<String> for RuntimeId {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
        {
            Ok(Self(value))
        } else {
            Err("invalid runtime identity")
        }
    }
}

/// Preserve an engine-issued token's representation; numeric producer epochs are not minted IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
pub enum RuntimeGeneration {
    Id(RuntimeId),
    Counter(RuntimeRevision),
}

/// SHA-256 binding over the producer's canonical operation/subject. Not authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(try_from = "String", into = "String")]
pub struct RuntimeDigest(
    #[schemars(length(min = 64, max = 64), regex(pattern = "^[a-f0-9]{64}$"))] String,
);
impl From<RuntimeDigest> for String {
    fn from(value: RuntimeDigest) -> Self {
        value.0
    }
}
impl TryFrom<String> for RuntimeDigest {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            Ok(Self(value))
        } else {
            Err("invalid runtime digest")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeCursor {
    pub generation: RuntimeGeneration,
    pub revision: RuntimeRevision,
}

/// Returned by the authenticated adapter, never accepted as caller authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeBinding {
    pub public_session_id: RuntimeId,
    pub native_session_id: RuntimeId,
    pub runtime_generation: RuntimeGeneration,
    pub controller_id: Option<RuntimeId>,
    pub controller_generation: Option<RuntimeGeneration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum RuntimeFamily {
    Commands,
    Queue,
    Permissions,
    PermissionConfig,
    Errors,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeUnavailableReason {
    SdkEngineAbsent,
    GeneratedDtoNotVendored,
    PortNotConfigured,
    EngineUnavailable,
    UnsupportedOperation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRejectCode {
    InvalidRequest,
    UnknownCommand,
    AmbiguousCommand,
    StaleGeneration,
    StaleRevision,
    UnknownRequest,
    AlreadyConsumed,
    Expired,
    Cancelled,
    AuthorityDenied,
    PolicyDenied,
    SubjectMismatch,
    JournalUnavailable,
    EngineFailure,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
#[ts(tag = "state", rename_all = "snake_case")]
pub enum RuntimeCapability {
    Available {
        family: RuntimeFamily,
    },
    Unavailable {
        family: RuntimeFamily,
        reason: RuntimeUnavailableReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCommandOrigin {
    Builtin,
    Extension,
    Prompt,
    Skill,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCatalogCoverage {
    Full,
    RegisteredOnly,
    Partial,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[ts(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeArgumentContract {
    RawString,
    JsonSchema { schema: serde_json::Value },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
#[ts(tag = "state", rename_all = "snake_case")]
pub enum RuntimeAvailability {
    Available,
    Unavailable { reason: RuntimeUnavailableReason },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeCommandDescriptor {
    pub command_id: RuntimeId,
    pub invocation: String,
    pub description: Option<String>,
    pub origin: RuntimeCommandOrigin,
    pub arguments: RuntimeArgumentContract,
    pub availability: RuntimeAvailability,
    pub required_interactions: Vec<RuntimeFamily>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeCommandCatalog {
    pub generation: RuntimeGeneration,
    pub coverage: RuntimeCatalogCoverage,
    pub commands: Vec<RuntimeCommandDescriptor>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum RuntimeCommandDisposition {
    Rejected,
    Handled,
    Queued,
    Started,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeCommandReceipt {
    pub operation_id: RuntimeId,
    pub command_id: RuntimeId,
    pub disposition: RuntimeCommandDisposition,
    pub rejection: Option<RuntimeRejectCode>,
}

/// Sanitized effect summary. Private task/title stay in the owning producer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSpawnSubject {
    pub kind: String,
    pub model: String,
    pub loadout: Option<String>,
    pub effect_digest: RuntimeDigest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePermissionOption {
    pub option_id: RuntimeId,
    pub model: Option<String>,
    pub effect_digest: RuntimeDigest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePermissionPresentation {
    pub handle_id: RuntimeId,
    pub request_id: RuntimeId,
    pub request_revision: RuntimeRevision,
    pub subject: Option<RuntimeSpawnSubject>,
    pub subject_digest: RuntimeDigest,
    pub operation_digest: RuntimeDigest,
    pub ceiling_digest: RuntimeDigest,
    pub expires_at_ms: RuntimeRevision,
    pub options: Vec<RuntimePermissionOption>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum RuntimePermissionDecision {
    AllowOnce,
    Deny,
    Cancel,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEffectState {
    NotStarted,
    Started,
    Succeeded,
    Failed,
    Cancelled,
    Uncertain,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePermissionReceiptKind {
    Decision,
    StartIntent,
    Result,
    Cancelled,
    Expired,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePermissionOutcome {
    Allow,
    Deny,
    Started,
    Completed,
    Failed,
    Cancelled,
    Expired,
    Uncertain,
}
/// Producer receipt identity is its issued generation + producer revision, not an invented ID.
/// Journaling acknowledgements do not imply fsync, native entry identity or effect completion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePermissionReceipt {
    pub cursor: RuntimeCursor,
    pub handle_id: RuntimeId,
    pub request_id: RuntimeId,
    pub request_revision: RuntimeRevision,
    pub kind: RuntimePermissionReceiptKind,
    pub outcome: RuntimePermissionOutcome,
    pub option_id: Option<RuntimeId>,
    pub reason: String,
    pub source: RuntimePermissionSource,
    pub controller_id: RuntimeId,
    pub native_session_digest: RuntimeDigest,
    pub subject_digest: RuntimeDigest,
    pub operation_digest: RuntimeDigest,
    pub at_ms: RuntimeRevision,
    pub restored: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePermissionSource {
    Tui,
    Rpc,
    Adapter,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePermissionSnapshot {
    pub cursor: RuntimeCursor,
    pub active: Vec<RuntimePermissionPresentation>,
    pub receipts: Vec<RuntimePermissionReceipt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum RuntimeAllowMode {
    Auto,
    Confirm,
    Timeout,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAutoDecision {
    Allow,
    Deny,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePermissionPolicy {
    pub cursor: RuntimeCursor,
    pub allow_mode: RuntimeAllowMode,
    pub auto_decision: RuntimeAutoDecision,
    pub allowed_models: Vec<String>,
    pub allowed_kinds: Vec<String>,
    pub max_subagents: u32,
    pub confirm_timeout_ms: RuntimeRevision,
}
/// Separate owner-authorized policy write; never carried in an answer payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "setting",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
#[ts(tag = "setting", content = "value", rename_all = "snake_case")]
pub enum RuntimePolicyChange {
    AllowMode(RuntimeAllowMode),
    AutoDecision(RuntimeAutoDecision),
    AllowedModels(Vec<String>),
    AllowedKinds(Vec<String>),
    MaxSubagents(u32),
    ConfirmTimeoutMs(RuntimeRevision),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeQueueCompleteness {
    Complete,
    Partial,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeQueueActionKind {
    Steer,
    FollowUp,
    CancelItem,
    Clear,
    UpdateItem,
    Reorder,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeQueueItem {
    pub item_id: Option<RuntimeId>,
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeQueueSnapshot {
    pub cursor: RuntimeCursor,
    pub completeness: RuntimeQueueCompleteness,
    pub steering: Vec<RuntimeQueueItem>,
    pub follow_up: Vec<RuntimeQueueItem>,
    pub supported_actions: Vec<RuntimeQueueActionKind>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[ts(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RuntimeQueueAction {
    Steer {
        text: String,
    },
    FollowUp {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    CancelItem {
        item_id: RuntimeId,
    },
    Clear,
    #[serde(rename_all = "camelCase")]
    UpdateItem {
        item_id: RuntimeId,
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    Reorder {
        item_ids: Vec<RuntimeId>,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeErrorSource {
    Command,
    Permission,
    Queue,
    Engine,
    Transport,
    Provider,
    Tool,
    Extension,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeError {
    pub event_id: RuntimeId,
    pub operation_id: Option<RuntimeId>,
    pub source: RuntimeErrorSource,
    pub code: String,
    pub message: String,
    pub phase: String,
    pub recoverable: bool,
    pub uncertain: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeErrorSnapshot {
    pub cursor: RuntimeCursor,
    pub errors: Vec<RuntimeError>,
}

/// Untrusted requests contain comparisons only; Session routing/actor authority is external.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "op",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[ts(tag = "op", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum RuntimeOperation {
    Capabilities,
    CommandCatalog,
    #[serde(rename_all = "camelCase")]
    CommandDispatch {
        catalog_generation: RuntimeGeneration,
        command_id: RuntimeId,
        args: String,
    },
    PermissionSnapshot,
    #[serde(rename_all = "camelCase")]
    PermissionDecide {
        producer_generation: RuntimeGeneration,
        handle_id: RuntimeId,
        request_revision: RuntimeRevision,
        controller_id: RuntimeId,
        controller_generation: RuntimeGeneration,
        decision: RuntimePermissionDecision,
        option_id: Option<RuntimeId>,
    },
    PolicyRead,
    PolicyApply {
        expected: RuntimeCursor,
        changes: Vec<RuntimePolicyChange>,
    },
    QueueSnapshot,
    QueueApply {
        expected: RuntimeCursor,
        action: RuntimeQueueAction,
    },
    #[serde(rename_all = "camelCase")]
    ErrorList {
        after_event_id: Option<RuntimeId>,
    },
    #[serde(rename_all = "camelCase")]
    ReceiptGet {
        operation_id: RuntimeId,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRequest {
    pub version: RuntimeVersion,
    pub request_id: RuntimeId,
    pub operation: RuntimeOperation,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[ts(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeResult {
    Capabilities {
        capabilities: Vec<RuntimeCapability>,
    },
    Catalog {
        catalog: RuntimeCommandCatalog,
    },
    CommandReceipt {
        receipt: RuntimeCommandReceipt,
    },
    Permissions {
        snapshot: RuntimePermissionSnapshot,
    },
    PermissionReceipt {
        receipt: RuntimePermissionReceipt,
    },
    Policy {
        policy: RuntimePermissionPolicy,
    },
    Queue {
        snapshot: RuntimeQueueSnapshot,
    },
    Errors {
        snapshot: RuntimeErrorSnapshot,
    },
    Unavailable {
        family: RuntimeFamily,
        reason: RuntimeUnavailableReason,
    },
    Rejected {
        code: RuntimeRejectCode,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeResponse {
    pub version: RuntimeVersion,
    pub request_id: RuntimeId,
    pub binding: RuntimeBinding,
    pub result: RuntimeResult,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[ts(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RuntimeEventPayload {
    Queue {
        snapshot: RuntimeQueueSnapshot,
    },
    Permissions {
        snapshot: RuntimePermissionSnapshot,
    },
    Error {
        error: RuntimeError,
    },
    #[serde(rename_all = "camelCase")]
    CommandOutcome {
        operation_id: RuntimeId,
        state: RuntimeEffectState,
        error_id: Option<RuntimeId>,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeEvent {
    pub version: RuntimeVersion,
    pub event_id: RuntimeId,
    pub binding: RuntimeBinding,
    pub payload: RuntimeEventPayload,
}

/// Native source identity is comparison evidence from a verified channel, never host authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSourceIdentity {
    pub native_session_id: RuntimeId,
    pub runtime_generation: RuntimeGeneration,
}

/// No public Session/controller binding crosses the native source boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSourceRequest {
    pub version: RuntimeVersion,
    pub request_id: RuntimeId,
    pub source: RuntimeSourceIdentity,
    pub operation: RuntimeOperation,
}

/// Source results are not RuntimeResponses: only the authenticated host adds a binding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSourceResponse {
    pub version: RuntimeVersion,
    pub request_id: RuntimeId,
    pub source: RuntimeSourceIdentity,
    pub result: RuntimeResult,
}

/// Pre-result refusal cannot fabricate a missing/stale source identity or host binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSourceStatusCode {
    InvalidRequest,
    NotNegotiated,
    SourceUnavailable,
    SourceChanged,
    AuthorityDenied,
    UnsupportedOperation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "type", deny_unknown_fields)]
#[ts(tag = "type")]
pub enum RuntimeSourceFrame {
    #[serde(rename = "interaction_source")]
    Request { request: RuntimeSourceRequest },
    #[serde(rename = "interaction_source_response")]
    Response { response: RuntimeSourceResponse },
    #[serde(rename = "interaction_source_status", rename_all = "camelCase")]
    #[ts(rename_all = "camelCase")]
    Status {
        request_id: Option<RuntimeId>,
        code: RuntimeSourceStatusCode,
    },
}

impl RuntimeSourceResponse {
    /// Correlation only. A matching source frame is not authenticated host admission.
    pub fn matches_request(&self, request: &RuntimeSourceRequest) -> bool {
        self.version == request.version
            && self.request_id == request.request_id
            && self.source == request.source
            && self.result.matches_operation(&request.operation)
    }
}

impl RuntimeResult {
    pub fn matches_operation(&self, operation: &RuntimeOperation) -> bool {
        use RuntimeOperation as Op;
        match (operation, self) {
            (_, Self::Rejected { .. }) => true,
            (Op::Capabilities, Self::Capabilities { .. })
            | (Op::CommandCatalog, Self::Catalog { .. })
            | (Op::PermissionSnapshot, Self::Permissions { .. })
            | (Op::PolicyRead | Op::PolicyApply { .. }, Self::Policy { .. })
            | (Op::QueueSnapshot | Op::QueueApply { .. }, Self::Queue { .. })
            | (Op::ErrorList { .. }, Self::Errors { .. }) => true,
            (Op::CommandDispatch { command_id, .. }, Self::CommandReceipt { receipt }) => {
                receipt.command_id == *command_id
            }
            (
                Op::PermissionDecide {
                    handle_id,
                    request_revision,
                    ..
                },
                Self::PermissionReceipt { receipt },
            ) => receipt.handle_id == *handle_id && receipt.request_revision == *request_revision,
            (Op::ReceiptGet { operation_id }, Self::CommandReceipt { receipt }) => {
                receipt.operation_id == *operation_id
            }
            (op, Self::Unavailable { family, .. }) => match family {
                RuntimeFamily::Commands => matches!(
                    op,
                    Op::CommandCatalog | Op::CommandDispatch { .. } | Op::ReceiptGet { .. }
                ),
                RuntimeFamily::Queue => matches!(op, Op::QueueSnapshot | Op::QueueApply { .. }),
                RuntimeFamily::Permissions => matches!(
                    op,
                    Op::PermissionSnapshot | Op::PermissionDecide { .. } | Op::ReceiptGet { .. }
                ),
                RuntimeFamily::PermissionConfig => {
                    matches!(op, Op::PolicyRead | Op::PolicyApply { .. })
                }
                RuntimeFamily::Errors => matches!(op, Op::ErrorList { .. }),
            },
            _ => false,
        }
    }
}

impl RuntimeOperation {
    /// Semantic CLI/API mapping; the CLI adapter must implement these same operations.
    pub fn cli_path(&self) -> &'static [&'static str] {
        match self {
            Self::Capabilities => &["runtime", "capabilities"],
            Self::CommandCatalog => &["runtime", "commands", "list"],
            Self::CommandDispatch { .. } => &["runtime", "commands", "run"],
            Self::PermissionSnapshot => &["runtime", "permissions", "list"],
            Self::PermissionDecide { .. } => &["runtime", "permissions", "decide"],
            Self::PolicyRead => &["runtime", "policy", "get"],
            Self::PolicyApply { .. } => &["runtime", "policy", "apply"],
            Self::QueueSnapshot => &["runtime", "queue", "get"],
            Self::QueueApply { .. } => &["runtime", "queue", "apply"],
            Self::ErrorList { .. } => &["runtime", "errors", "list"],
            Self::ReceiptGet { .. } => &["runtime", "receipts", "get"],
        }
    }
}

fn decode_frame<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    if bytes.len() > MAX_RUNTIME_FRAME_BYTES {
        return Err("runtime frame exceeds limit".into());
    }
    // Never echo untrusted arguments, queue text or provider details in a decode error.
    serde_json::from_slice(bytes).map_err(|_| "invalid runtime frame".into())
}

pub fn decode_runtime_request(bytes: &[u8]) -> Result<RuntimeRequest, String> {
    decode_frame(bytes)
}

pub fn decode_runtime_response(bytes: &[u8]) -> Result<RuntimeResponse, String> {
    decode_frame(bytes)
}

pub fn decode_runtime_event(bytes: &[u8]) -> Result<RuntimeEvent, String> {
    decode_frame(bytes)
}

pub fn decode_runtime_source_frame(bytes: &[u8]) -> Result<RuntimeSourceFrame, String> {
    decode_frame(bytes)
}

#[cfg(test)]
mod tests;
