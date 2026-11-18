//! Client runtime for `oqto.session-view` (ADR-0087 decision 2): the sans-IO
//! [`client::SessionClient`] (attachment, outbox with request ids and receipts, instant send,
//! paging, purge rules), the encrypted [`cache::FileCache`], the byoagnt WebSocket
//! [`transport`], and the async [`runtime`] that joins them. Shared by the terminal client,
//! Desktop and Mobile.

pub mod cache;
pub mod client;
pub mod runtime;
pub mod transport;

pub use cache::{CacheRecord, FileCache, KeyKind, KeySource};
pub use client::{Item, Notice, Options, Request, SessionClient, Tracked};
pub use runtime::{Link, SessionConfig, SessionHandle, Shared};
pub use transport::Secret;
