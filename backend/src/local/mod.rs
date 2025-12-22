//! Local runtime module for running services directly on the host.
//!
//! This module provides an alternative to container-based sessions for single-user
//! setups (e.g., Proxmox LXC, local development). It spawns opencode, fileserver,
//! and ttyd as native processes instead of inside containers.
//!
//! ## Linux User Isolation
//!
//! In multi-user mode, the local runtime can create and manage Linux users for
//! each platform user, providing proper process isolation:
//!
//! - Each platform user gets a dedicated Linux user (e.g., `octo_alice`)
//! - Processes run under the user's UID/GID
//! - Home directories are owned by the respective user
//! - Requires root/sudo privileges for user creation

mod linux_users;
mod process;
mod runtime;

pub use linux_users::LinuxUsersConfig;
#[allow(unused_imports)]
pub use process::{ProcessHandle, ProcessManager};
pub use runtime::{LocalRuntime, LocalRuntimeConfig};
