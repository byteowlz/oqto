use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct RunnerUserConfig {
    pub fileserver_binary: String,
    pub ttyd_binary: String,
    pub pi_binary: String,
    pub runner_id: String,
    pub workspace_dir: PathBuf,
    pub pi_sessions_dir: PathBuf,
    pub memories_dir: PathBuf,
    pub single_user: bool,
    pub linux_users_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct ConfigFile {
    local: LocalSection,
    runner: RunnerSection,
    pi: PiSection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct PiSection {
    executable: String,
}

impl Default for PiSection {
    fn default() -> Self {
        Self {
            executable: "pi".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct LocalSection {
    fileserver_binary: String,
    ttyd_binary: String,
    workspace_dir: String,
    single_user: bool,
    linux_users: LinuxUsersSection,
}

impl Default for LocalSection {
    fn default() -> Self {
        Self {
            fileserver_binary: "oqto-files".to_string(),
            ttyd_binary: "ttyd".to_string(),
            workspace_dir: "~/oqto".to_string(),
            single_user: false,
            linux_users: LinuxUsersSection::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct LinuxUsersSection {
    enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct RunnerSection {
    runner_id: Option<String>,
    pi_sessions_dir: Option<String>,
    memories_dir: Option<String>,
}

impl RunnerUserConfig {
    pub fn load() -> Self {
        Self::load_from_path(Self::default_config_path())
    }

    pub fn default_config_path() -> PathBuf {
        let config_dir = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                PathBuf::from(home).join(".config")
            });
        config_dir.join("oqto").join("config.toml")
    }

    pub fn load_from_path(path: PathBuf) -> Self {
        let config_file: ConfigFile = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(contents) => match toml::from_str(&contents) {
                    Ok(config) => {
                        info!("Loaded config from {:?}", path);
                        config
                    }
                    Err(e) => {
                        warn!("Failed to parse config {:?}: {}, using defaults", path, e);
                        ConfigFile::default()
                    }
                },
                Err(e) => {
                    warn!("Failed to read config {:?}: {}, using defaults", path, e);
                    ConfigFile::default()
                }
            }
        } else {
            debug!("Config file {:?} not found, using defaults", path);
            ConfigFile::default()
        };

        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let data_dir = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(&home).join(".local").join("share"));

        let (pi_binary, pi_version) = resolve_pi_binary(&config_file.pi.executable, &home);
        info!("Pi binary: {} (v{})", pi_binary, pi_version);

        let runner_id = config_file
            .runner
            .runner_id
            .or_else(|| std::env::var("OQTO_RUNNER_ID").ok())
            .or_else(|| std::env::var("HOSTNAME").ok())
            .unwrap_or_else(|| "local".to_string());

        info!("Runner ID: {}", runner_id);

        Self {
            fileserver_binary: config_file.local.fileserver_binary,
            ttyd_binary: config_file.local.ttyd_binary,
            pi_binary,
            runner_id,
            workspace_dir: Self::expand_path(&config_file.local.workspace_dir, &home),
            pi_sessions_dir: config_file
                .runner
                .pi_sessions_dir
                .map(|p| Self::expand_path(&p, &home))
                .unwrap_or_else(|| {
                    PathBuf::from(&home)
                        .join(".pi")
                        .join("agent")
                        .join("sessions")
                }),
            memories_dir: config_file
                .runner
                .memories_dir
                .map(|p| Self::expand_path(&p, &home))
                .unwrap_or_else(|| data_dir.join("mmry")),
            single_user: config_file.local.single_user,
            linux_users_enabled: config_file.local.linux_users.enabled,
        }
    }

    fn expand_path(path: &str, home: &str) -> PathBuf {
        if path.starts_with("~/") {
            PathBuf::from(path.replacen("~", home, 1))
        } else if path.starts_with("$HOME") {
            PathBuf::from(path.replacen("$HOME", home, 1))
        } else {
            PathBuf::from(path)
        }
    }
}

/// Resolve the Pi binary the runner should spawn.
///
/// If `configured` is a path (contains `/`), it is honored verbatim. A bare
/// name is resolved via `which`, falling back to common system locations.
///
/// Setup (`scripts/setup/05-install-core.sh`) installs Pi at
/// `/usr/local/lib/pi-coding-agent` with a self-link in its own
/// `node_modules/@mariozechner/pi-coding-agent` so user extensions can resolve
/// the host package. As long as setup ran, the system wrapper at
/// `/usr/local/bin/pi` is the correct binary regardless of any per-user
/// `~/.bun/bin/pi` that may also exist.
fn resolve_pi_binary(configured: &str, _home: &str) -> (String, String) {
    let chosen = if configured.contains('/') {
        configured.to_string()
    } else if let Ok(output) = std::process::Command::new("which").arg(configured).output()
        && output.status.success()
    {
        let resolved = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if resolved.is_empty() {
            fallback_system_pi(configured)
        } else {
            resolved
        }
    } else {
        fallback_system_pi(configured)
    };

    let version =
        pi_binary_version(std::path::Path::new(&chosen)).unwrap_or_else(|| "unknown".to_string());
    (chosen, version)
}

fn fallback_system_pi(configured: &str) -> String {
    for candidate in ["/usr/local/bin/pi", "/usr/bin/pi"] {
        if PathBuf::from(candidate).exists() {
            return candidate.to_string();
        }
    }
    warn!("Pi not found in PATH or at /usr/local/bin/pi -- run setup.sh to install");
    configured.to_string()
}

fn pi_binary_version(path: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new(path)
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    // Pi installs differ in where they write `--version`: the bun-global
    // shim writes to stderr (node shebang), while the system bash wrapper
    // forwards bun's stdout. Accept either stream, preferring stdout.
    for stream in [&output.stdout, &output.stderr] {
        let v = String::from_utf8_lossy(stream).trim().to_string();
        if !v.is_empty() {
            return Some(v);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_section_defaults_do_not_enable_linux_isolation() {
        let cfg = ConfigFile::default();
        assert!(!cfg.local.single_user);
        assert!(!cfg.local.linux_users.enabled);
    }

    #[test]
    fn local_section_parses_single_user_and_linux_users_enabled() {
        let toml = r#"
            [local]
            single_user = true

            [local.linux_users]
            enabled = true
        "#;

        let parsed: ConfigFile = toml::from_str(toml).expect("parse config");
        assert!(parsed.local.single_user);
        assert!(parsed.local.linux_users.enabled);
    }
}
