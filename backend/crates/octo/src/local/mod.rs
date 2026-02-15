//! Local runtime module for running services directly on the host.
//!
//! This module provides an alternative to container-based sessions for single-user
//! setups (e.g., Proxmox LXC, local development). It spawns fileserver,
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
//!
//! ## Sandboxing
//!
//! Optional bubblewrap-based sandboxing adds namespace isolation:
//!
//! - Mount namespace: only specified paths are visible
//! - PID namespace: process can't see other processes
//! - Network namespace: optionally isolated
//! - Protects sensitive files (~/.ssh, ~/.aws, etc.)

mod linux_users;
mod process;
mod runtime;
mod sandbox;
mod user_hstry;
mod user_mmry;
mod user_sldr;

pub use linux_users::LinuxUsersConfig;
#[allow(unused_imports)]
pub use process::{
    ProcessHandle, ProcessManager, are_ports_available, find_process_on_port, force_kill_process,
    is_port_available, kill_process,
};
pub use runtime::{LocalRuntime, LocalRuntimeConfig};
#[allow(unused_imports)]
pub use sandbox::{
    GuardConfig, GuardPolicy, NetworkConfig, NetworkMode, PromptConfig, SandboxConfig,
    SandboxConfigFile, SandboxProfile, SshProxyConfig,
};
pub use user_hstry::{UserHstryConfig, UserHstryManager};
pub use user_mmry::{UserMmryConfig, UserMmryManager};
pub use user_sldr::{UserSldrConfig, UserSldrManager};
