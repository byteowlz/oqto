use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

#[derive(Parser)]
#[command(
    name = "oqto-setup",
    about = "Plan and hydrate Oqto setup from typed install contracts"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the expected provisioning contract for an install profile.
    Plan {
        /// Install profile to plan: personal or team.
        #[arg(long, default_value = "personal")]
        profile: SetupProfile,
        /// Output machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    Hydrate {
        /// Path to the install config (oqto.install.toml)
        #[arg(long, default_value = "oqto.install.toml")]
        install_config: PathBuf,
        /// Override hydration mode (merge or overwrite)
        #[arg(long)]
        mode: Option<HydrateMode>,
    },
    /// Install a release artifact using transactional activation.
    Install {
        /// Path to release tarball (e.g. oqto-<version>-<target>.tar.gz)
        #[arg(long)]
        artifact: PathBuf,
        /// Optional path to sha256 file for the artifact.
        #[arg(long)]
        checksum: Option<PathBuf>,
        /// Releases root directory.
        #[arg(long, default_value = "/var/lib/oqto/releases")]
        releases_root: PathBuf,
        /// Stable binary link directory.
        #[arg(long, default_value = "/usr/local/bin")]
        bin_dir: PathBuf,
        /// Run strict doctor check after activation.
        #[arg(long, default_value_t = true)]
        doctor_strict: bool,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum HydrateMode {
    Merge,
    Overwrite,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum SetupProfile {
    Personal,
    Team,
}

impl From<SetupProfile> for oqto_provisioning::InstallProfile {
    fn from(value: SetupProfile) -> Self {
        match value {
            SetupProfile::Personal => Self::Personal,
            SetupProfile::Team => Self::Team,
        }
    }
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
        Command::Plan { profile, json } => print_plan(profile, json),
        Command::Hydrate {
            install_config,
            mode,
        } => hydrate_configs(&install_config, mode),
        Command::Install {
            artifact,
            checksum,
            releases_root,
            bin_dir,
            doctor_strict,
        } => install_release(
            &artifact,
            checksum.as_deref(),
            &releases_root,
            &bin_dir,
            doctor_strict,
        ),
    }
}

fn print_plan(profile: SetupProfile, json: bool) -> Result<()> {
    let manifest = oqto_provisioning::manifest(profile.into());

    if json {
        println!("{}", serde_json::to_string_pretty(&manifest)?);
        return Ok(());
    }

    println!("Oqto setup plan: {}", manifest.summary);
    println!("Runner socket: {}", manifest.runner_socket.pattern);
    println!("\nPaths:");
    for path in &manifest.paths {
        println!(
            "- {} owner={} group={} mode={} -- {}",
            path.path, path.owner, path.group, path.mode, path.purpose
        );
    }
    println!("\nServices:");
    for service in &manifest.services {
        let user = service.user.as_deref().unwrap_or("root/system");
        println!(
            "- {} user={} enabled={} active={} -- {}",
            service.name, user, service.enabled, service.active, service.purpose
        );
    }
    println!("\nDeclared checks (static; severity shown only if the check fails):");
    for check in &manifest.checks {
        println!(
            "- severity-if-failed={:?}: {} -- remediation: {}",
            check.severity, check.description, check.remediation
        );
    }

    Ok(())
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

fn install_release(
    artifact: &Path,
    checksum: Option<&Path>,
    releases_root: &Path,
    bin_dir: &Path,
    doctor_strict: bool,
) -> Result<()> {
    if !artifact.exists() {
        anyhow::bail!("Artifact not found: {}", artifact.display());
    }

    if let Some(checksum_path) = checksum {
        verify_checksum(artifact, checksum_path)?;
    }

    fs::create_dir_all(releases_root)
        .with_context(|| format!("Failed creating releases root {}", releases_root.display()))?;

    let release_id = release_id_from_artifact(artifact)?;
    let release_dir = releases_root.join(&release_id);

    if release_dir.exists() {
        fs::remove_dir_all(&release_dir).with_context(|| {
            format!(
                "Failed removing existing release dir {}",
                release_dir.display()
            )
        })?;
    }
    fs::create_dir_all(&release_dir)
        .with_context(|| format!("Failed creating release dir {}", release_dir.display()))?;

    extract_tarball(artifact, &release_dir)?;

    let bin_src = release_dir.join("immutable/bin");
    if !bin_src.exists() {
        anyhow::bail!("Invalid artifact layout: missing {}", bin_src.display());
    }

    let current_link = releases_root.join("current");
    swap_current_symlink(&current_link, &release_dir)?;
    relink_bins(&current_link.join("immutable/bin"), bin_dir)?;

    if doctor_strict {
        let _ = run_doctor_strict();
    }

    println!("Installed release {}", release_id);
    Ok(())
}

fn verify_checksum(artifact: &Path, checksum_path: &Path) -> Result<()> {
    let expected = fs::read_to_string(checksum_path)
        .with_context(|| format!("Failed to read checksum file {}", checksum_path.display()))?
        .split_whitespace()
        .next()
        .map(ToString::to_string)
        .context("Checksum file missing hash")?;

    let output = ProcessCommand::new("sha256sum")
        .arg(artifact)
        .output()
        .context("failed to run sha256sum")?;
    if !output.status.success() {
        anyhow::bail!("sha256sum failed for {}", artifact.display());
    }
    let actual = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .map(ToString::to_string)
        .context("Unable to parse sha256sum output")?;

    if actual != expected {
        anyhow::bail!(
            "Checksum mismatch for {} (expected {}, got {})",
            artifact.display(),
            expected,
            actual
        );
    }

    Ok(())
}

fn release_id_from_artifact(artifact: &Path) -> Result<String> {
    let file = artifact
        .file_name()
        .and_then(|s| s.to_str())
        .context("invalid artifact filename")?;
    let id = file.trim_end_matches(".tar.gz");
    if id.is_empty() {
        anyhow::bail!("unable to derive release id from artifact filename");
    }
    Ok(id.to_string())
}

fn extract_tarball(artifact: &Path, dst: &Path) -> Result<()> {
    let output = ProcessCommand::new("tar")
        .arg("-xzf")
        .arg(artifact)
        .arg("-C")
        .arg(dst)
        .output()
        .context("failed to run tar")?;
    if !output.status.success() {
        anyhow::bail!("failed to extract artifact {}", artifact.display());
    }

    let mut entries = fs::read_dir(dst)
        .with_context(|| format!("Failed to read extracted dir {}", dst.display()))?;
    let first = entries
        .next()
        .transpose()?
        .map(|e| e.path())
        .context("artifact extracted empty directory")?;

    if first.is_dir() {
        for child in fs::read_dir(&first)? {
            let child = child?;
            let name = child.file_name();
            let target = dst.join(name);
            fs::rename(child.path(), target)?;
        }
        fs::remove_dir_all(first)?;
    }

    Ok(())
}

fn swap_current_symlink(current_link: &Path, release_dir: &Path) -> Result<()> {
    let tmp_link = current_link.with_extension("tmp");
    if tmp_link.exists() {
        fs::remove_file(&tmp_link)?;
    }
    symlink(release_dir, &tmp_link).with_context(|| {
        format!(
            "Failed to create temporary symlink {} -> {}",
            tmp_link.display(),
            release_dir.display()
        )
    })?;
    fs::rename(&tmp_link, current_link).with_context(|| {
        format!(
            "Failed to atomically update {} to {}",
            current_link.display(),
            release_dir.display()
        )
    })?;
    Ok(())
}

fn relink_bins(bin_src: &Path, bin_dir: &Path) -> Result<()> {
    fs::create_dir_all(bin_dir)
        .with_context(|| format!("Failed creating bin dir {}", bin_dir.display()))?;

    for entry in fs::read_dir(bin_src)
        .with_context(|| format!("Failed reading bin source dir {}", bin_src.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name();
        let dst = bin_dir.join(name);
        if let Ok(meta) = fs::symlink_metadata(&dst) {
            if meta.file_type().is_dir() {
                fs::remove_dir_all(&dst)?;
            } else {
                fs::remove_file(&dst)?;
            }
        }
        symlink(&path, &dst)
            .with_context(|| format!("Failed linking {} -> {}", dst.display(), path.display()))?;
    }
    Ok(())
}

fn run_doctor_strict() -> Result<()> {
    let output = ProcessCommand::new("oqtoctl")
        .args(["doctor", "--contract", "--profile", "auto", "--strict"])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            println!("Post-activation doctor strict passed");
            Ok(())
        }
        Ok(_) => anyhow::bail!("Post-activation doctor strict failed"),
        Err(_) => {
            println!("Warning: oqtoctl not available; skipping post-activation doctor");
            Ok(())
        }
    }
}
