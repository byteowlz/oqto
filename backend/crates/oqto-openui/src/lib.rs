//! OpenUI Lang parts for native clients (ADR-0074, ADR-0087 decision 8): a bounded parser
//! ([`lang`]) and the Oqto profile's core set with the native-or-fallback decision
//! ([`profile`]). Shared by the terminal client, Desktop and Mobile; no IO, no rendering.
//! Nothing here evaluates, calls or fetches anything, and rendering a part grants nothing.

pub mod lang;
pub mod profile;

pub use lang::{Bounds, Expr, LangError};
pub use profile::{Decision, Fallback, Node, Value, decide, decide_body};
