//! Tier definitions and configuration loading.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::Tier;

/// Complete scaffold configuration with all tier definitions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScaffoldConfig {
    /// Templates directory path.
    #[serde(default)]
    pub templates_path: Option<String>,

    /// Tier definitions.
    #[serde(default)]
    pub tiers: TierDefinitions,
}

/// Definitions for all tiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierDefinitions {
    pub private: TierConfig,
    pub normal: TierConfig,
    pub privileged: TierConfig,
}

/// Configuration for a single tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    /// Human-readable description.
    pub description: String,

    /// Sandbox configuration.
    pub sandbox: SandboxConfig,

    /// OpenCode configuration.
    pub opencode: OpencodeConfig,
}

/// Sandbox configuration for a tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_development")]
    pub profile: String,
    #[serde(default)]
    pub deny_read: Vec<String>,
    #[serde(default)]
    pub allow_write: Vec<String>,
    #[serde(default)]
    pub deny_write: Vec<String>,
    #[serde(default)]
    pub isolate_network: bool,
    #[serde(default)]
    pub isolate_pid: bool,
}

fn default_true() -> bool {
    true
}

fn default_development() -> String {
    "development".to_string()
}

/// OpenCode configuration for a tier.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpencodeConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_providers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_providers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<HashMap<String, ProviderConfig>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whitelist: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blacklist: Option<Vec<String>>,
}

impl Default for TierDefinitions {
    fn default() -> Self {
        Self {
            private: TierConfig::default_private(),
            normal: TierConfig::default_normal(),
            privileged: TierConfig::default_privileged(),
        }
    }
}

impl TierConfig {
    fn default_private() -> Self {
        Self {
            description:
                "Restricted access: local models only, network isolation, project directory only"
                    .to_string(),
            sandbox: SandboxConfig {
                enabled: true,
                profile: "strict".to_string(),
                deny_read: vec![
                    "~/.ssh".to_string(),
                    "~/.gnupg".to_string(),
                    "~/.aws".to_string(),
                    "~/.config/gcloud".to_string(),
                    "~/.azure".to_string(),
                ],
                allow_write: vec!["/tmp".to_string()],
                deny_write: vec!["~/.config/octo/sandbox.toml".to_string()],
                isolate_network: true,
                isolate_pid: true,
            },
            opencode: OpencodeConfig {
                enabled_providers: Some(vec!["ollama".to_string(), "lmstudio".to_string()]),
                disabled_providers: None,
                provider: None,
            },
        }
    }

    fn default_normal() -> Self {
        let mut providers = HashMap::new();
        providers.insert(
            "anthropic".to_string(),
            ProviderConfig {
                whitelist: Some(vec![
                    "claude-sonnet-4-20250514".to_string(),
                    "claude-haiku-3-5-20241022".to_string(),
                ]),
                blacklist: None,
            },
        );
        providers.insert(
            "openai".to_string(),
            ProviderConfig {
                whitelist: Some(vec!["gpt-4o".to_string(), "gpt-4o-mini".to_string()]),
                blacklist: None,
            },
        );

        Self {
            description: "Standard access: cloud LLMs allowed, project directory only".to_string(),
            sandbox: SandboxConfig {
                enabled: true,
                profile: "development".to_string(),
                deny_read: vec![
                    "~/.ssh".to_string(),
                    "~/.gnupg".to_string(),
                    "~/.aws".to_string(),
                ],
                allow_write: vec![
                    "~/.cargo".to_string(),
                    "~/.rustup".to_string(),
                    "~/.local/bin".to_string(),
                    "~/.npm".to_string(),
                    "~/.bun".to_string(),
                    "/tmp".to_string(),
                ],
                deny_write: vec!["~/.config/octo/sandbox.toml".to_string()],
                isolate_network: false,
                isolate_pid: true,
            },
            opencode: OpencodeConfig {
                enabled_providers: Some(vec![
                    "anthropic".to_string(),
                    "openai".to_string(),
                    "ollama".to_string(),
                ]),
                disabled_providers: None,
                provider: Some(providers),
            },
        }
    }

    fn default_privileged() -> Self {
        Self {
            description: "Full access: all models, minimal restrictions".to_string(),
            sandbox: SandboxConfig {
                enabled: true,
                profile: "minimal".to_string(),
                deny_read: vec!["~/.ssh".to_string(), "~/.gnupg".to_string()],
                allow_write: vec![
                    "~/.cargo".to_string(),
                    "~/.rustup".to_string(),
                    "~/.local/bin".to_string(),
                    "~/.npm".to_string(),
                    "~/.bun".to_string(),
                    "~/.config".to_string(),
                    "/tmp".to_string(),
                ],
                deny_write: vec!["~/.config/octo/sandbox.toml".to_string()],
                isolate_network: false,
                isolate_pid: false,
            },
            opencode: OpencodeConfig {
                enabled_providers: None,
                disabled_providers: None,
                provider: None,
            },
        }
    }
}

impl ScaffoldConfig {
    /// Load configuration from files and environment.
    ///
    /// Priority (highest to lowest):
    /// 1. OCTO_SCAFFOLD_CONFIG environment variable (path to config file)
    /// 2. ~/.config/octo/scaffold.toml
    /// 3. /etc/octo/scaffold.toml
    /// 4. Built-in defaults
    pub fn load() -> Result<Self> {
        let mut config = Self::default();

        // Try /etc/octo/scaffold.toml first (lowest priority)
        let system_config = Path::new("/etc/octo/scaffold.toml");
        if system_config.exists() {
            debug!("Loading system config from {:?}", system_config);
            config = Self::merge(config, Self::load_file(system_config)?);
        }

        // Try ~/.config/octo/scaffold.toml
        if let Some(user_config) = Self::user_config_path()
            && user_config.exists()
        {
            debug!("Loading user config from {:?}", user_config);
            config = Self::merge(config, Self::load_file(&user_config)?);
        }

        // Try OCTO_SCAFFOLD_CONFIG environment variable (highest priority)
        if let Ok(env_path) = std::env::var("OCTO_SCAFFOLD_CONFIG") {
            let path = PathBuf::from(shellexpand::tilde(&env_path).to_string());
            if path.exists() {
                info!("Loading config from OCTO_SCAFFOLD_CONFIG: {:?}", path);
                config = Self::merge(config, Self::load_file(&path)?);
            } else {
                anyhow::bail!("Config file not found: {:?}", path);
            }
        }

        Ok(config)
    }

    fn user_config_path() -> Option<PathBuf> {
        dirs_next::config_dir().map(|p| p.join("octo").join("scaffold.toml"))
    }

    fn load_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("reading config file: {:?}", path))?;
        let config: Self =
            toml::from_str(&content).with_context(|| format!("parsing config file: {:?}", path))?;
        Ok(config)
    }

    fn merge(base: Self, overlay: Self) -> Self {
        Self {
            templates_path: overlay.templates_path.or(base.templates_path),
            tiers: TierDefinitions {
                private: overlay.tiers.private,
                normal: overlay.tiers.normal,
                privileged: overlay.tiers.privileged,
            },
        }
    }

    /// Get the tier configuration for a given tier.
    pub fn get_tier(&self, tier: Tier) -> &TierConfig {
        match tier {
            Tier::Private => &self.tiers.private,
            Tier::Normal => &self.tiers.normal,
            Tier::Privileged => &self.tiers.privileged,
        }
    }

    /// Get the templates path from config or environment.
    pub fn templates_path(&self) -> Option<PathBuf> {
        // Environment variable takes precedence
        if let Ok(path) = std::env::var("OCTO_TEMPLATES_PATH") {
            let expanded = shellexpand::tilde(&path).to_string();
            return Some(PathBuf::from(expanded));
        }

        // Then config file
        if let Some(path) = &self.templates_path {
            let expanded = shellexpand::tilde(path).to_string();
            return Some(PathBuf::from(expanded));
        }

        // Default location
        dirs_next::data_dir().map(|p| p.join("octo").join("templates"))
    }

    /// Generate an example configuration file.
    pub fn example_config() -> String {
        let config = Self::default();
        toml::to_string_pretty(&config).unwrap_or_default()
    }
}

/// List all available tiers with their descriptions.
pub fn list_tiers(config: &ScaffoldConfig) {
    println!("Available permission tiers:\n");

    for (name, tier_enum) in [
        ("private", Tier::Private),
        ("normal", Tier::Normal),
        ("privileged", Tier::Privileged),
    ] {
        let tier = config.get_tier(tier_enum);
        println!("  {} - {}", name, tier.description);

        println!("    Sandbox:");
        println!("      Network isolation: {}", tier.sandbox.isolate_network);
        println!("      PID isolation: {}", tier.sandbox.isolate_pid);

        if let Some(providers) = &tier.opencode.enabled_providers {
            println!("    Providers: {}", providers.join(", "));
        } else {
            println!("    Providers: all");
        }
        println!();
    }
}
