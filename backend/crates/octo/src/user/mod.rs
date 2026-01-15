//! User management module.
//!
//! Provides user CRUD operations, authentication support, and role management.

#![allow(dead_code)]

mod models;
mod repository;
mod service;

#[allow(unused_imports)]
pub use models::{CreateUserRequest, UpdateUserRequest, User, UserInfo, UserListQuery, UserRole};
pub use repository::UserRepository;
pub use service::{UserService, UserStats};
