//! Client runtime for `oqto.session-view` (ADR-0087 decision 2): the sans-IO
//! [`client::SessionClient`] (attachment, outbox with request ids and receipts, instant send,
//! paging, purge rules), the encrypted [`cache::FileCache`], the byoagnt WebSocket
//! [`transport`], and the async [`runtime`] that joins them. Shared by the terminal client,
//! Desktop and Mobile.
//!
//! Reconnect waits for a correlated attachment before effects. Requests retain their original
//! [`RequestSource`]; replacement stores quarantine pending operations rather than replaying
//! or falsely settling them. [`SessionClient::uncertain_requests`] exposes these outcomes.
//! Cached requests without a durable store binding are uncertain even if marked unsent: a
//! crash may have followed first send but preceded its last save. Denial still purges private
//! state and emits an outcome-unknown notice; it does not cancel independently admitted work.

pub mod cache;
pub mod client;
pub mod runtime;
pub mod transport;

pub use cache::{CacheRecord, FileCache, KeyKind, KeySource};
pub use client::{Item, Notice, Options, Request, RequestSource, SessionClient, Tracked};
pub use runtime::{Link, SessionConfig, SessionHandle, Shared};
pub use transport::Secret;
