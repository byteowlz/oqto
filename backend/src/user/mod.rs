//! User management module.
//!
//! Provides user CRUD operations, authentication support, and role management.

mod models;
mod repository;
mod service;

pub use models::{
    CreateUserRequest, UpdateUserRequest, User, UserInfo, UserListQuery, UserRole,
};
pub use repository::UserRepository;
pub use service::{UserService, UserStats};
