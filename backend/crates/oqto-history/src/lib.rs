//! History and oqto-log storage boundaries.
//!
//! This crate is being extracted incrementally from the server crate so runner
//! code can depend on history storage without depending on the `oqto` binary
//! crate.

pub mod oqto_log;
