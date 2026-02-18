use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "oqto-setup",
    about = "Hydrate Oqto config files from an install config"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Hydrate {
        /// Path to the install config (oqto.install.toml)
        #[arg(long, default_value = "oqto.install.toml")]
        install_config: PathBuf,
        /// Override hydration mode (merge or overwrite)
        #[arg(long)]
        mode: Option<HydrateMode>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum HydrateMode {
    Merge,
    Overwrite,
}

#[derive(Debug, Default, Deserialize)]
struct InstallConfig {
    #[serde(default)]
    install: InstallSection,
    #[serde(default)]
    oqto: Option<toml::Value>,
    #[serde(default)]
    sandbox: Option<toml::Value>,
    #[serde(default)]
    hstry: Option<toml::Value>,
    #[serde(default)]
    mmry: Option<toml::Value>,
}

#[derive(Debug, Default, Deserialize)]
struct InstallSection {
    mode: Option<HydrateMode>,
    config_home: Option<String>,
    data_home: Option<String>,
    state_home: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Hydrate {
            install_config,
            mode,
        } => hydrate_configs(&install_config, mode),
    }
}

fn hydrate_configs(install_path: &Path, mode_override: Option<HydrateMode>) -> Result<()> {
    let contents = fs::read_to_string(install_path)
        .with_context(|| format!("Failed to read install config: {}", install_path.display()))?;
    let install_config: InstallConfig =
        toml::from_str(&contents).context("Failed to parse install config")?;

    let xdg = XdgDefaults::new()?;
    let config_home = resolve_path(install_config.install.config_home.as_deref(), &xdg.config)
        .context("Failed to resolve config_home")?;
    let _data_home = resolve_path(install_config.install.data_home.as_deref(), &xdg.data)
        .context("Failed to resolve data_home")?;
    let _state_home = resolve_path(install_config.install.state_home.as_deref(), &xdg.state)
        .context("Failed to resolve state_home")?;

    let mode = mode_override
        .or(install_config.install.mode)
        .unwrap_or(HydrateMode::Merge);

    let targets = [
        (
            "oqto",
            install_config.oqto,
            config_home.join("oqto").join("config.toml"),
        ),
        (
            "sandbox",
            install_config.sandbox,
            config_home.join("oqto").join("sandbox.toml"),
        ),
        (
            "hstry",
            install_config.hstry,
            config_home.join("hstry").join("config.toml"),
        ),
        (
            "mmry",
            install_config.mmry,
            config_home.join("mmry").join("config.toml"),
        ),
    ];

    for (label, config, path) in targets {
        if let Some(value) = config {
            write_config_file(&path, value, mode)
                .with_context(|| format!("Failed to write {} config", label))?;
            println!("Wrote {} config to {}", label, path.display());
        } else {
            println!("Skipping {} config (not provided)", label);
        }
    }

    Ok(())
}

struct XdgDefaults {
    config: PathBuf,
    data: PathBuf,
    state: PathBuf,
}

impl XdgDefaults {
    fn new() -> Result<Self> {
        let home = home_dir()?;
        let config = env_or_default_path("XDG_CONFIG_HOME", home.join(".config"));
        let data = env_or_default_path("XDG_DATA_HOME", home.join(".local/share"));
        let state = env_or_default_path("XDG_STATE_HOME", home.join(".local/state"));

        Ok(Self {
            config,
            data,
            state,
        })
    }

    fn expand_context(&self) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        vars.insert(
            "XDG_CONFIG_HOME".to_string(),
            self.config.display().to_string(),
        );
        vars.insert("XDG_DATA_HOME".to_string(), self.data.display().to_string());
        vars.insert(
            "XDG_STATE_HOME".to_string(),
            self.state.display().to_string(),
        );
        if let Ok(home) = std::env::var("HOME") {
            vars.insert("HOME".to_string(), home);
        }
        vars
    }
}

fn resolve_path(input: Option<&str>, default: &Path) -> Result<PathBuf> {
    if let Some(raw) = input {
        let defaults = XdgDefaults::new()?;
        let expanded = expand_with_defaults(raw, &defaults.expand_context())?;
        Ok(PathBuf::from(expanded))
    } else {
        Ok(default.to_path_buf())
    }
}

fn expand_with_defaults(raw: &str, defaults: &HashMap<String, String>) -> Result<String> {
    let expanded = shellexpand::env_with_context(raw, |key| {
        Ok::<Option<String>, std::env::VarError>(
            defaults
                .get(key)
                .cloned()
                .or_else(|| std::env::var(key).ok()),
        )
    })
    .context("Failed to expand environment variables")?;

    let expanded = shellexpand::tilde(&expanded).to_string();
    Ok(expanded)
}

fn env_or_default_path(env_var: &str, default: PathBuf) -> PathBuf {
    match std::env::var(env_var) {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => default,
    }
}

fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().context("Failed to resolve home directory")
}

fn write_config_file(path: &Path, config: toml::Value, mode: HydrateMode) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let final_value = match mode {
        HydrateMode::Overwrite => config,
        HydrateMode::Merge => {
            if path.exists() {
                let existing_contents = fs::read_to_string(path).with_context(|| {
                    format!("Failed to read existing config: {}", path.display())
                })?;
                let existing_value: toml::Value = toml::from_str(&existing_contents)
                    .context("Failed to parse existing config")?;
                merge_values(existing_value, config)
            } else {
                config
            }
        }
    };

    let rendered = toml::to_string_pretty(&final_value).context("Failed to render config")?;
    fs::write(path, rendered)
        .with_context(|| format!("Failed to write config: {}", path.display()))?;

    Ok(())
}

fn merge_values(existing: toml::Value, updates: toml::Value) -> toml::Value {
    match (existing, updates) {
        (toml::Value::Table(mut existing_table), toml::Value::Table(update_table)) => {
            merge_tables(&mut existing_table, update_table);
            toml::Value::Table(existing_table)
        }
        (_, update) => update,
    }
}

fn merge_tables(target: &mut toml::value::Table, updates: toml::value::Table) {
    for (key, update_value) in updates {
        match target.remove(&key) {
            Some(existing_value) => {
                let merged = merge_values(existing_value, update_value);
                target.insert(key, merged);
            }
            None => {
                target.insert(key, update_value);
            }
        }
    }
}
