use anyhow::Result;
use log::{debug, error, info, warn};
use oqto_sandbox::{SandboxConfig, SandboxConfigFile};
use std::path::{Path, PathBuf};

/// Default socket path pattern.
///
/// This mirrors the backend client default while runner ownership is being
/// migrated. Once the client/protocol boundary moves, this constant should be
/// shared from this crate instead of duplicated in the server crate.
const DEFAULT_SOCKET_PATTERN: &str = "{runtime_dir}/oqto-runner.sock";

pub fn get_default_socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(runtime_dir).join(DEFAULT_SOCKET_PATTERN)
}

pub fn load_sandbox_config(
    no_sandbox: bool,
    sandbox_config_path: Option<&PathBuf>,
    allow_user_fallback: bool,
) -> Result<Option<SandboxConfig>> {
    if no_sandbox {
        info!("Sandboxing disabled via --no-sandbox flag");
        return Ok(None);
    }

    if let Some(config_path) = sandbox_config_path {
        let contents = std::fs::read_to_string(config_path)?;
        let file: SandboxConfigFile = toml::from_str(&contents)?;
        let mut config: SandboxConfig = file.into();
        config.enabled = true;
        info!("Loaded sandbox config from {:?}", config_path);
        return Ok(Some(config));
    }

    let system_path = Path::new("/etc/oqto/sandbox.toml");
    let user_path = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".config")
        })
        .join("oqto")
        .join("sandbox.toml");

    let mut candidates: Vec<&Path> = vec![system_path];
    if allow_user_fallback {
        candidates.push(&user_path);
        info!("Sandbox config lookup mode: system+user fallback (single-user mode)");
    } else {
        info!("Sandbox config lookup mode: system-only (fail-closed)");
    }

    for config_path in candidates {
        if !config_path.exists() {
            continue;
        }
        match std::fs::read_to_string(config_path) {
            Ok(contents) => match toml::from_str::<SandboxConfigFile>(&contents) {
                Ok(file) => {
                    let config: SandboxConfig = file.into();
                    if config.enabled {
                        info!(
                            "Loaded sandbox config from {}, profile='{}'",
                            config_path.display(),
                            config.profile
                        );
                        return Ok(Some(config));
                    }
                    info!(
                        "Sandbox config at {} exists but is disabled (enabled=false)",
                        config_path.display()
                    );
                    return Ok(None);
                }
                Err(e) => {
                    warn!(
                        "Failed to parse sandbox config {}: {}. Trying next.",
                        config_path.display(),
                        e
                    );
                }
            },
            Err(e) => {
                warn!(
                    "Failed to read sandbox config {}: {}. Trying next.",
                    config_path.display(),
                    e
                );
            }
        }
    }

    if allow_user_fallback {
        Ok(None)
    } else {
        Err(anyhow::anyhow!(
            "No valid sandbox config found at /etc/oqto/sandbox.toml (system-only mode). \
             Either install a system config or start runner with --sandbox-config."
        ))
    }
}

pub fn log_sandbox_state(sandbox_config: &Option<SandboxConfig>) {
    if sandbox_config.is_some() {
        info!("Sandbox enabled - processes will be wrapped with bwrap");
    } else {
        warn!("Sandbox disabled - processes will run without isolation");
    }
}

pub fn load_env_file() {
    let env_path = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        format!("{}/.config", home)
    }) + "/oqto/env";

    let path = std::path::Path::new(&env_path);
    if !path.exists() {
        debug!("No env file at {}, skipping", env_path);
        return;
    }

    match std::fs::read_to_string(path) {
        Ok(contents) => {
            let mut count = 0;
            for line in contents.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = trimmed.split_once('=') {
                    let key = key.trim();
                    let value = value.trim();
                    if !key.is_empty() {
                        unsafe { std::env::set_var(key, value) };
                        count += 1;
                    }
                }
            }
            info!("Loaded {} environment variables from {}", count, env_path);
        }
        Err(e) => {
            error!("Failed to read env file {}: {}", env_path, e);
        }
    }
}

/// First file descriptor passed by systemd-style socket activation.
const SD_LISTEN_FDS_START: i32 = 3;

/// Adopt a Unix listener inherited via the systemd LISTEN_FDS protocol
/// (podman-quadlet socket activation). Returns None when not activated;
/// errors on a malformed or unsupported activation environment rather than
/// silently binding a second socket.
pub fn inherited_unix_listener() -> anyhow::Result<Option<std::os::unix::net::UnixListener>> {
    let Some(fds) = std::env::var_os("LISTEN_FDS") else {
        return Ok(None);
    };
    let fds: i32 = fds
        .to_string_lossy()
        .parse()
        .map_err(|_| anyhow::anyhow!("LISTEN_FDS is not a number"))?;
    validate_activation(
        fds,
        std::env::var("LISTEN_PID").ok().as_deref(),
        std::process::id(),
    )?;
    // SAFETY: the activation manager passed fd 3 open and close-on-exec
    // cleared; validate_activation confirmed it is addressed to this process.
    let listener = unsafe {
        use std::os::fd::FromRawFd;
        std::os::unix::net::UnixListener::from_raw_fd(SD_LISTEN_FDS_START)
    };
    Ok(Some(listener))
}

fn validate_activation(fds: i32, listen_pid: Option<&str>, my_pid: u32) -> anyhow::Result<()> {
    let pid: u32 = listen_pid
        .ok_or_else(|| anyhow::anyhow!("LISTEN_FDS set without LISTEN_PID"))?
        .parse()
        .map_err(|_| anyhow::anyhow!("LISTEN_PID is not a pid"))?;
    if pid != my_pid {
        anyhow::bail!("LISTEN_PID {pid} does not match this process ({my_pid})");
    }
    if fds != 1 {
        anyhow::bail!("expected exactly one activated socket, got {fds}");
    }
    Ok(())
}

#[cfg(test)]
mod activation_tests {
    use super::validate_activation;

    #[test]
    fn activation_validation_is_strict() {
        assert!(validate_activation(1, Some("42"), 42).is_ok());
        assert!(validate_activation(1, None, 42).is_err());
        assert!(validate_activation(1, Some("41"), 42).is_err());
        assert!(validate_activation(2, Some("42"), 42).is_err());
        assert!(validate_activation(0, Some("42"), 42).is_err());
        assert!(validate_activation(1, Some("nope"), 42).is_err());
    }
}
