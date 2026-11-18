//! `oqto.session-view` v0 (revision 2.2) for native clients: the terminal client, Desktop and
//! Mobile share this crate (ADR-0087 decision 2).
//!
//! - [`frames`]: strict frame shapes of `frames.schema.json`.
//! - [`ops`]: upstream Chord's seven op verbs, applied all or nothing.
//! - [`view`]: one attachment's view with readers for the stable subset.
//! - [`transcript`]: the cached transcript with gaps and earlier pages.
//! - [`store`]: the cache key.
//! - [`parts`]: generative UI parts (`oqto.ui` entries, revision 2.2).
//!
//! No IO, no async, no terminal code. Conformance against the contract bundle lives in
//! `tests/conformance.rs`.

pub mod canonical;
pub mod frames;
pub mod ops;
pub mod parts;
pub mod store;
pub mod transcript;
pub mod view;

pub use frames::{ClientFrame, Entry, HostFrame, Receipt, ReceiptStatus};
pub use store::CacheKey;
pub use transcript::{Gap, Row, Transcript, TranscriptRecord};
pub use view::{View, ViewError};
