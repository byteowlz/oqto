use anyhow::Result;
use log::{debug, error, info, warn};
use oqto_sandbox::SandboxConfig;
use std::path::{Path, PathBuf};

use crate::runner::client::DEFAULT_SOCKET_PATTERN;

pub fn get_default_socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(runtime_dir).join(DEFAULT_SOCKET_PATTERN)
}

pub fn load_sandbox_config(
    no_sandbox: bool,
    sandbox_config_path: Option<&PathBuf>,
) -> Result<Option<SandboxConfig>> {
    if no_sandbox {
        info!("Sandboxing disabled via --no-sandbox flag");
        return Ok(None);
    }

    if let Some(config_path) = sandbox_config_path {
        let contents = std::fs::read_to_string(config_path)?;
        let mut config: SandboxConfig = toml::from_str(&contents)?;
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

    let candidates: &[&Path] = &[system_path, &user_path];
    for config_path in candidates {
        if !config_path.exists() {
            continue;
        }
        match std::fs::read_to_string(config_path) {
            Ok(contents) => match toml::from_str::<SandboxConfig>(&contents) {
                Ok(config) => {
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

    Ok(None)
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
