//! Host OS integration for Oqto.
//!
//! This crate owns native host concerns: Linux users, process management,
//! sandbox configuration, and local host runtime orchestration.

pub mod linux_users;
pub mod process;
pub mod runtime;
pub mod sandbox;

pub use linux_users::LinuxUsersConfig;
pub use process::{
    ProcessHandle, ProcessManager, are_ports_available, base_system_env, find_process_on_port,
    force_kill_process, is_port_available, kill_process,
};
pub use runtime::{LocalRuntime, LocalRuntimeConfig};
pub use sandbox::{
    GuardConfig, GuardPolicy, NetworkConfig, NetworkMode, PromptConfig, SandboxConfig,
    SandboxConfigFile, SandboxProfile, SshProxyConfig,
};
