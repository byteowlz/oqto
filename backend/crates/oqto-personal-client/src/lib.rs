//! Personal-host client extracted from Mobile Core, without UI, FFI, Runner or
//! legacy Server protocol dependencies. Stores are platform-owned; transport,
//! signing and current client admission have one implementation here.
mod client;
mod credential;
mod dto;
mod error;
mod history;
mod relations;
mod sessions;
mod transport;

pub use client::{CredentialStore, HostStore, PersonalClient, pairing_preview};
pub use dto::*;
pub use error::{ClientError, Result};
pub use oqto_personal_wire::actions::{HostCapabilities, SessionInput};
pub use oqto_personal_wire::validate_origin;
pub use oqto_session_relations::{
    Relation as SessionRelation, RelationKind as SessionRelationKind,
};
