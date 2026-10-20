use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod probe;

#[derive(Debug, Clone, Default)]
pub struct RunnerUserConfig {
    pub credentials: crate::credentials::CredentialConfig,
    pub provider_login: Option<crate::provider_login::ProviderLoginConfig>,
    pub history_read: Option<crate::history_read::HistoryReadConfig>,
    /// Resolved once from explicit owner startup flags, never Account config.
    pub personal_source: Option<std::sync::Arc<crate::personal_source::PersonalSource>>,
    pub fileserver_binary: String,
    pub ttyd_binary: String,
    pub pi_binary: String,
    pub runner_id: String,
    pub workspace_dir: PathBuf,
    pub pi_sessions_dir: PathBuf,
    pub memories_dir: PathBuf,
    pub single_user: bool,
    pub linux_users_enabled: bool,
    pub terminal_enabled: bool,
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
    terminal_enabled: bool,
    linux_users: LinuxUsersSection,
}

impl Default for LocalSection {
    fn default() -> Self {
        Self {
            fileserver_binary: "oqto-files".to_string(),
            ttyd_binary: "ttyd".to_string(),
            workspace_dir: "~/oqto".to_string(),
            single_user: false,
            terminal_enabled: true,
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
    credentials: crate::credentials::CredentialConfig,
    provider_login: Option<crate::provider_login::ProviderLoginConfig>,
    history_read: Option<crate::history_read::HistoryReadConfig>,
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

        let pi_binary = resolve_pi_binary(&config_file.pi.executable, &home, &data_dir);
        info!("Selected Pi binary: {} (not yet qualified)", pi_binary);

        let runner_id = config_file
            .runner
            .runner_id
            .or_else(|| std::env::var("OQTO_RUNNER_ID").ok())
            .or_else(|| std::env::var("HOSTNAME").ok())
            .unwrap_or_else(|| "local".to_string());

        info!("Runner ID: {}", runner_id);

        Self {
            credentials: config_file.runner.credentials,
            provider_login: config_file.runner.provider_login,
            history_read: config_file.runner.history_read,
            personal_source: None,
            fileserver_binary: config_file.local.fileserver_binary,
            ttyd_binary: config_file.local.ttyd_binary,
            terminal_enabled: config_file.local.terminal_enabled,
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

    /// Qualify only the selected runtime. Metadata-only maintenance commands do
    /// not execute Pi; daemon readiness must call this after ownership checks.
    pub fn qualify_pi(&self, workdir: &Path) -> anyhow::Result<()> {
        probe::qualify(Path::new(&self.pi_binary), Some(workdir))
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
/// If `configured` is an explicit path (contains `/`), it is honored first.
/// Otherwise the runner prefers Oqto's checksummed, atomically promoted
/// standalone runtime. Legacy system paths remain migration fallbacks; a
/// user-global package-manager install is deliberately not a candidate because
/// its independently resolved dependencies are not part of the deployed release.
fn resolve_pi_binary(configured: &str, home: &str, data_dir: &Path) -> String {
    let candidates = pi_binary_candidates(configured, home, data_dir);
    candidates
        .iter()
        .find(|candidate| !candidate.contains('/') || Path::new(candidate).exists())
        .or_else(|| candidates.first())
        .cloned()
        .unwrap_or_else(|| configured.to_string())
}

fn pi_binary_candidates(configured: &str, _home: &str, data_dir: &Path) -> Vec<String> {
    if configured == "pi"
        && let Some(runtime) = std::env::current_exe()
            .ok()
            .and_then(|exe| crate::pi_manager::bundled_pi_runtime(&exe))
    {
        // One release-owned candidate: even a broken package must not cause a
        // silent downgrade to an unrelated user or global Pi installation.
        return vec![runtime.to_string_lossy().into_owned()];
    }
    let mut candidates = Vec::new();
    if configured.contains('/') {
        return vec![configured.to_string()];
    } else {
        push_candidate(
            &mut candidates,
            Some(
                data_dir
                    .join("oqto/pi-runtimes/current/pi")
                    .to_string_lossy()
                    .to_string(),
            ),
        );
        push_candidate(
            &mut candidates,
            Some("/var/lib/oqto/pi-runtimes/current/pi".to_string()),
        );
        // The default bare name must not resolve through the invoking user's
        // package-manager PATH. Custom bare executable names remain supported.
        if configured != "pi" {
            push_candidate(&mut candidates, resolve_configured_pi(configured));
        }
    }
    push_candidate(&mut candidates, Some("/usr/local/bin/pi".to_string()));
    push_candidate(&mut candidates, Some("/usr/bin/pi".to_string()));
    candidates
}

fn resolve_configured_pi(configured: &str) -> Option<String> {
    if configured.contains('/') {
        return Some(configured.to_string());
    }

    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(configured))
                .find(|path| path.is_file())
                .map(|path| path.to_string_lossy().into_owned())
        })
        .or_else(|| Some(configured.to_string()))
}

fn push_candidate(candidates: &mut Vec<String>, candidate: Option<String>) {
    let Some(candidate) = candidate else {
        return;
    };
    if !candidates.iter().any(|existing| existing == &candidate) {
        candidates.push(candidate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_selection_does_not_execute_pi() -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir()?;
        let executable = temp.path().join("pi");
        let calls = temp.path().join("invocations");
        std::fs::write(
            &executable,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nif [ \"$1\" = --version ]; then printf 'synthetic-version\\n'; exit 0; fi\nexec /bin/sleep 10\n",
                calls.display()
            ),
        )?;
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))?;
        let config = temp.path().join("config.toml");
        std::fs::write(
            &config,
            format!("[pi]\nexecutable={}\n", serde_json::to_string(&executable)?),
        )?;
        let loaded = RunnerUserConfig::load_from_path(config);
        assert_eq!(loaded.pi_binary, executable.to_string_lossy());
        assert!(!calls.exists());
        // Managed creation is covered by pi_startup_readiness: correlated typed
        // get_state + actual-cwd model validation, not an alive-only smoke probe.
        Ok(())
    }

    #[test]
    fn bare_pi_prefers_canonical_runtime_and_excludes_user_global_install() {
        let candidates =
            pi_binary_candidates("pi", "/home/alice", Path::new("/home/alice/.local/share"));
        assert_eq!(
            candidates.first().map(String::as_str),
            Some("/home/alice/.local/share/oqto/pi-runtimes/current/pi")
        );
        assert_eq!(
            candidates.get(1).map(String::as_str),
            Some("/var/lib/oqto/pi-runtimes/current/pi")
        );
        assert!(!candidates.iter().any(|path| path.contains("/.bun/")));
    }

    #[test]
    fn explicit_pi_path_remains_first_candidate() {
        let candidates = pi_binary_candidates(
            "/opt/custom/pi",
            "/home/alice",
            Path::new("/home/alice/.local/share"),
        );
        assert_eq!(
            candidates.first().map(String::as_str),
            Some("/opt/custom/pi")
        );
    }

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
