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
}

impl Default for LocalSection {
    fn default() -> Self {
        Self {
            fileserver_binary: "oqto-files".to_string(),
            ttyd_binary: "ttyd".to_string(),
            workspace_dir: "~/oqto".to_string(),
        }
    }
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

        let pi_binary = {
            let configured = &config_file.pi.executable;
            if configured.contains('/') {
                configured.clone()
            } else {
                let system_paths = ["/usr/local/bin/pi", "/usr/bin/pi"];
                let system_pi = system_paths
                    .iter()
                    .find(|p| PathBuf::from(p).exists())
                    .map(|p| p.to_string());

                if let Some(path) = system_pi {
                    path
                } else {
                    match std::process::Command::new("which").arg(configured).output() {
                        Ok(output) if output.status.success() => {
                            String::from_utf8_lossy(&output.stdout).trim().to_string()
                        }
                        _ => {
                            warn!(
                                "Pi not found at system paths or in PATH. Run setup.sh to install."
                            );
                            configured.clone()
                        }
                    }
                }
            }
        };

        let pi_version = std::process::Command::new(&pi_binary)
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| {
                o.status
                    .success()
                    .then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
            })
            .unwrap_or_else(|| "unknown".to_string());
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
