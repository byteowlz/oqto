//! Compatibility shim: sandbox types now live in the dedicated `oqto-sandbox` crate.

pub use oqto_sandbox::{
    GuardConfig, GuardPolicy, NetworkConfig, NetworkMode, PromptConfig, SandboxConfig,
    SandboxConfigFile, SandboxProfile, SshProxyConfig,
};
