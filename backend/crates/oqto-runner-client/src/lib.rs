//! Host-neutral consumer state over the shared runtime-interaction contract.
//!
//! One canonical dialect, consumed directly by Oq surfaces, oqt's terminal and
//! headless callers, mobile and desktop. This crate owns typed validation,
//! request construction, correlation and renderer-independent state - never
//! authority: producers (personal bridge, managed Runner, oqt host/guard) own
//! admission, queues, receipts and history.
//!
//! Transport, authentication, engine ports and sockets are deliberately absent,
//! so nothing here can depend on Pi, a TUI, or a running producer.
#![forbid(unsafe_code)]

pub use oqto_protocol::runtime_interactions as contract;

mod catalog;
mod error;
mod permissions;
mod queue;
mod receipts;
mod session;

pub use catalog::CatalogState;
pub use error::ClientError;
pub use permissions::PermissionState;
pub use queue::QueueState;
pub use receipts::{ReceiptEntry, ReceiptLog, RecordedReceipt};
pub use session::{Applied, InteractionSession, RequestIdSource};

/// Environment override naming a vendored bundle directory, used by tests and
/// verifier tooling. Runtimes never read the bundle from disk.
pub const BUNDLE_ENV: &str = "OQTO_RUNTIME_INTERACTIONS_BUNDLE";

/// Default workspace bundle path, for tests and tooling only.
#[must_use]
pub fn default_bundle_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../oqto-protocol/contracts/runtime-interactions/v1")
}
