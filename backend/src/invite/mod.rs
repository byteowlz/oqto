//! Invite code module for self-service registration.
//!
//! Provides invite code generation, validation, and management for
//! controlling user registration.

mod models;
mod repository;

pub use models::{
    BatchCreateInviteCodesRequest, CreateInviteCodeRequest, InviteCode, InviteCodeListQuery,
    InviteCodeSummary,
};
pub use repository::InviteCodeRepository;
