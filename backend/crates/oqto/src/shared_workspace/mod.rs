//! Shared workspace module.
//!
//! Provides shared workspace CRUD, membership management, USERS.md generation,
//! and user-prefixed prompt support for multi-user collaboration.

mod models;
mod repository;
mod service;
mod users_md;

pub use models::{
    AddMemberRequest, AdminSharedWorkspaceInfo, ConvertToSharedRequest,
    CreateSharedWorkspaceRequest, CreateSharedWorkspaceWorkdirRequest, MemberRole, SharedWorkspace,
    SharedWorkspaceInfo, SharedWorkspaceMember, SharedWorkspaceMemberInfo,
    TransferOwnershipRequest, UpdateMemberRequest, UpdateSharedWorkspaceRequest,
};
pub use repository::SharedWorkspaceRepository;
pub use service::SharedWorkspaceService;
pub use users_md::generate_users_md;
