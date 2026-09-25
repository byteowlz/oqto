use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Component, Path, PathBuf};
use std::process::Command as ProcessCommand;

mod acquire;
mod deps;

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
        /// SHA-256 file independently obtained for the exact artifact.
        #[arg(long)]
        checksum: PathBuf,
        /// Releases root directory.
        #[arg(long, default_value = "/var/lib/oqto/releases")]
        releases_root: PathBuf,
        /// Stable binary link directory.
        #[arg(long, default_value = "/usr/local/bin")]
        bin_dir: PathBuf,
        /// Run strict doctor check after activation. Pass `--doctor-strict false`
        /// when a deploy orchestrator starts services + validates health itself
        /// (the strict gate requires services already active).
        #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
        doctor_strict: bool,
        /// Number of superseded releases to retain when pruning. `current` and
        /// `last-good` are always preserved on top of this. 0 disables pruning.
        #[arg(long, default_value_t = 3)]
        keep_releases: usize,
    },
    /// Resolve the binary-acquisition set from a dependency manifest and print
    /// each component's release artifact URL (ADR-0018 / vemr.9).
    Deps {
        /// Path to the dependency manifest (dependencies.toml).
        #[arg(long, default_value = "dependencies.toml")]
        manifest: PathBuf,
        /// Target architecture for the artifact bundle.
        #[arg(long, default_value = "x86-64")]
        arch: ArchArg,
        /// Base GitHub org URL the artifacts are published under.
        #[arg(long, default_value = "https://github.com/byteowlz")]
        base_url: String,
        /// Output machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Download and checksum-verify the binary-acquisition set into a staging
    /// directory (ADR-0018 / vemr.9). The single acquisition path replacing the
    /// duplicate download logic in install.sh / docker / setup / deploy.
    Acquire {
        /// Path to the dependency manifest (dependencies.toml).
        #[arg(long, default_value = "dependencies.toml")]
        manifest: PathBuf,
        /// Target architecture for the artifact bundle.
        #[arg(long, default_value = "x86-64")]
        arch: ArchArg,
        /// Base GitHub org URL the artifacts are published under.
        #[arg(long, default_value = "https://github.com/byteowlz")]
        base_url: String,
        /// Directory to stage downloaded artifacts into.
        #[arg(long, default_value = "dist/out")]
        dest: PathBuf,
        /// If set, extract the staged bundle and install binaries into this dir
        /// (e.g. /usr/local/bin) — completes the acquire -> install path.
        #[arg(long)]
        install_bin: Option<PathBuf>,
        /// Acquire only the byteowlz tools, excluding the oqto platform bundle
        /// (which is deployed separately via `oqto-setup install`). Used by
        /// deploy-time dependency remediation.
        #[arg(long)]
        tools_only: bool,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ArchArg {
    #[value(name = "x86-64", alias = "x86_64")]
    X86_64,
    #[value(name = "aarch64", alias = "arm64")]
    Aarch64,
}

impl From<ArchArg> for deps::Arch {
    fn from(value: ArchArg) -> Self {
        match value {
            ArchArg::X86_64 => Self::X86_64,
            ArchArg::Aarch64 => Self::Aarch64,
        }
    }
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
            keep_releases,
        } => install_release(
            &artifact,
            &checksum,
            &releases_root,
            &bin_dir,
            doctor_strict,
            keep_releases,
        ),
        Command::Deps {
            manifest,
            arch,
            base_url,
            json,
        } => resolve_deps(&manifest, arch, &base_url, json),
        Command::Acquire {
            manifest,
            arch,
            base_url,
            dest,
            install_bin,
            tools_only,
        } => acquire_bundle(
            &manifest,
            arch,
            &base_url,
            &dest,
            install_bin.as_deref(),
            tools_only,
        ),
    }
}

/// Resolve the dependency manifest and download + checksum-verify every artifact
/// into `dest` using the real `curl` fetcher.
fn acquire_bundle(
    manifest: &Path,
    arch: ArchArg,
    base_url: &str,
    dest: &Path,
    install_bin: Option<&Path>,
    tools_only: bool,
) -> Result<()> {
    let contents = fs::read_to_string(manifest)
        .with_context(|| format!("Failed to read dependency manifest: {}", manifest.display()))?;
    let mut components = deps::parse_dependency_manifest(&contents)?;
    if tools_only {
        // Drop the oqto platform bundle entirely: dependency remediation only
        // needs the byteowlz tools, and must not fail (or re-download the large
        // bundle) on account of the oqto release. oqto is deployed separately via
        // `oqto-setup install`.
        components.retain(|c| c.name != "oqto");
    }
    let target = deps::Arch::from(arch).target();
    let plan = deps::plan_downloads(&components, base_url, target);

    let staged = acquire::acquire_artifacts(&plan, dest, &acquire::CurlFetcher)?;
    for path in &staged {
        println!("staged {}", path.display());
    }
    println!(
        "Acquired {} artifact(s) into {}",
        staged.len(),
        dest.display()
    );

    if let Some(bin) = install_bin {
        // The oqto platform bundle is a structured release installed via
        // `oqto-setup install` (transactional); flat-install only the tools.
        let tool_tarballs: Vec<PathBuf> = components
            .iter()
            .zip(&staged)
            .filter(|(c, _)| c.name != "oqto")
            .map(|(_, p)| p.clone())
            .collect();
        let installed = acquire::install_staged(&tool_tarballs, bin)?;
        for b in &installed {
            println!("installed {} -> {}", b, bin.join(b).display());
        }
        println!(
            "Installed {} binary/binaries into {}",
            installed.len(),
            bin.display()
        );
    }
    Ok(())
}

/// Read a dependency manifest and print the release artifact URL for each
/// component in the acquisition set. The single manifest-driven view that the
/// duplicate acquisition paths converge on (ADR-0018 / vemr.9).
fn resolve_deps(manifest: &Path, arch: ArchArg, base_url: &str, json: bool) -> Result<()> {
    let contents = fs::read_to_string(manifest)
        .with_context(|| format!("Failed to read dependency manifest: {}", manifest.display()))?;
    let components = deps::parse_dependency_manifest(&contents)?;
    let target = deps::Arch::from(arch).target();
    let plan = deps::plan_downloads(&components, base_url, target);

    if json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        for artifact in &plan {
            println!(
                "{:<12} v{:<10} {}",
                artifact.name, artifact.version, artifact.url
            );
        }
    }

    Ok(())
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

/// Stage and transactionally activate a release artifact: verify -> extract ->
/// activate. Acquisition (checksum + tarball extraction) is separated from
/// activation so the transaction itself stays subprocess-free and testable.
fn install_release(
    artifact: &Path,
    checksum: &Path,
    releases_root: &Path,
    bin_dir: &Path,
    doctor_strict: bool,
    keep_releases: usize,
) -> Result<()> {
    if !artifact.exists() {
        anyhow::bail!("Artifact not found: {}", artifact.display());
    }

    verify_checksum(artifact, checksum)?;

    fs::create_dir_all(releases_root)
        .with_context(|| format!("Failed creating releases root {}", releases_root.display()))?;

    let release_id = release_id_from_artifact(artifact)?;
    let release_dir = releases_root.join(&release_id);

    if ["current", "last-good"].iter().any(|link| {
        read_link_target(&releases_root.join(link)).as_deref() == Some(release_dir.as_path())
    }) {
        anyhow::bail!(
            "refusing to overwrite an active or last-good release: {}",
            release_dir.display()
        );
    }
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

    if let Err(extraction_error) = extract_tarball(artifact, &release_dir) {
        let referenced = ["current", "last-good"].iter().any(|link| {
            read_link_target(&releases_root.join(link)).as_deref() == Some(release_dir.as_path())
        });
        if referenced {
            anyhow::bail!(
                "Extraction failed ({extraction_error}); staged release unexpectedly referenced; \
                 preserved for diagnosis"
            );
        }
        if let Err(cleanup_error) = fs::remove_dir_all(&release_dir) {
            anyhow::bail!(
                "Extraction failed ({extraction_error}); also failed removing stage {}: {cleanup_error}",
                release_dir.display()
            );
        }
        return Err(extraction_error);
    }

    if let Err(activation_error) = activate_release(
        &release_dir,
        releases_root,
        bin_dir,
        doctor_strict,
        keep_releases,
    ) {
        // A failed staged release is disposable only after current/last-good
        // have been restored. Preserve it for diagnosis if rollback failed.
        let referenced = ["current", "last-good"].iter().any(|link| {
            read_link_target(&releases_root.join(link)).as_deref() == Some(release_dir.as_path())
        });
        if !referenced && let Err(cleanup_error) = fs::remove_dir_all(&release_dir) {
            anyhow::bail!(
                "{activation_error}; also failed removing unreferenced stage {}: {cleanup_error}",
                release_dir.display()
            );
        }
        return Err(activation_error);
    }

    println!("Installed release {}", release_id);
    Ok(())
}

/// Transactionally activate an already-staged release directory.
///
/// Mirrors the ADR-0016 activation contract (and replaces the parallel bash in
/// `scripts/deploy.sh`, see ADR-0018 / vemr.3): validate -> atomically switch
/// `current` -> relink bins -> strict-doctor gate. On a failed gate the previous
/// release is restored (rollback); on success `last-good` is advanced and
/// superseded releases pruned (`current`/`last-good` always preserved).
fn activate_release(
    release_dir: &Path,
    releases_root: &Path,
    bin_dir: &Path,
    doctor_strict: bool,
    keep_releases: usize,
) -> Result<()> {
    validate_staged_release(release_dir)?;

    // Record what `current` points at *before* the switch so a failed activation
    // can be rolled back to it (mirrors deploy.sh rollback_host).
    let current_link = releases_root.join("current");
    let previous = read_link_target(&current_link);
    validate_bin_destinations(
        &release_dir.join("immutable/bin"),
        bin_dir,
        &current_link,
        previous.as_deref(),
    )?;

    atomic_symlink(&current_link, release_dir)?;
    let activation = (|| {
        relink_bins(&current_link.join("immutable/bin"), bin_dir)?;
        if doctor_strict {
            run_doctor_strict(release_dir)?;
        }
        Ok::<(), anyhow::Error>(())
    })();
    if let Err(cause) = activation {
        match previous.as_ref() {
            Some(prev) => {
                // Report BOTH errors if rollback also fails; never hide the
                // original relink or doctor failure.
                let rollback = atomic_symlink(&current_link, prev)
                    .and_then(|_| relink_bins(&current_link.join("immutable/bin"), bin_dir));
                match rollback {
                    Ok(()) => anyhow::bail!(
                        "Activation failed ({cause}); rolled back to {}",
                        prev.display()
                    ),
                    Err(rb_err) => anyhow::bail!(
                        "Activation failed ({cause}); rollback to {} ALSO failed \
                         ({rb_err}) — bins may be inconsistent, manual intervention needed",
                        prev.display()
                    ),
                }
            }
            None => match rollback_fresh_activation(&current_link, release_dir, bin_dir) {
                Ok(()) => anyhow::bail!(
                    "Activation failed ({cause}); removed fresh release pointer and entrypoints"
                ),
                Err(rollback_err) => anyhow::bail!(
                    "Activation failed ({cause}); fresh rollback ALSO failed ({rollback_err}) \
                     — release pointers or entrypoints may be inconsistent"
                ),
            },
        }
    }

    // Activation confirmed good: advance last-good and prune superseded releases.
    atomic_symlink(&releases_root.join("last-good"), release_dir)?;
    let pruned = prune_old_releases(releases_root, keep_releases)?;
    if !pruned.is_empty() {
        println!(
            "Pruned {} old release(s): {}",
            pruned.len(),
            pruned.join(", ")
        );
    }

    Ok(())
}

/// A release can only take over entrypoints belonging to the active release.
/// Refuse arbitrary files, directories and unrelated symlinks before changing
/// either `current` or a destination in /usr/local/bin.
fn validate_bin_destinations(
    bin_src: &Path,
    bin_dir: &Path,
    current: &Path,
    previous: Option<&Path>,
) -> Result<()> {
    for entry in fs::read_dir(bin_src)? {
        let entry = entry?;
        if !entry.path().is_file() {
            continue;
        }
        let dst = bin_dir.join(entry.file_name());
        match fs::symlink_metadata(&dst) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(meta) if previous.is_some() && meta.file_type().is_symlink() => {
                let expected = current.join("immutable/bin").join(entry.file_name());
                if fs::read_link(&dst)? != expected {
                    anyhow::bail!("refusing to replace unmanaged entrypoint {}", dst.display());
                }
            }
            Ok(_) => anyhow::bail!("refusing to replace unmanaged entrypoint {}", dst.display()),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

/// Undo only entrypoints created by this fresh activation. Never unlink an
/// unrelated file or symlink if the directory changed during activation.
fn rollback_fresh_activation(current: &Path, release: &Path, bin_dir: &Path) -> Result<()> {
    if read_link_target(current).as_deref() != Some(release) {
        anyhow::bail!("current no longer points at the failed release");
    }
    for entry in fs::read_dir(release.join("immutable/bin"))? {
        let entry = entry?;
        if !entry.path().is_file() {
            continue;
        }
        let dst = bin_dir.join(entry.file_name());
        match fs::symlink_metadata(&dst) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let expected = current.join("immutable/bin").join(entry.file_name());
                if fs::read_link(&dst)? != expected {
                    anyhow::bail!(
                        "managed entrypoint changed during rollback: {}",
                        dst.display()
                    );
                }
                fs::remove_file(&dst)?;
            }
            Ok(_) => anyhow::bail!("unexpected entrypoint during rollback: {}", dst.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    fs::remove_file(current)?;
    Ok(())
}

/// Only the explicit full release is activatable until the runner-only
/// installer/doctor contract exists. This gate runs before switching `current`
/// even when doctor is deferred by an orchestrator.
fn validate_staged_release(release_dir: &Path) -> Result<()> {
    const REQUIRED_BINS: [&str; 8] = [
        "oqto",
        "oqtoctl",
        "oqto-setup",
        "oqto-runner",
        "oqto-files",
        "oqto-sandbox",
        "oqto-usermgr",
        "pi-bridge",
    ];
    let manifest_path = release_dir.join("manifest.toml");
    let manifest_meta = fs::symlink_metadata(&manifest_path)
        .with_context(|| format!("Invalid artifact: missing {}", manifest_path.display()))?;
    if !manifest_meta.file_type().is_file() {
        anyhow::bail!("Invalid artifact: manifest must be a regular file");
    }
    let contents = fs::read_to_string(&manifest_path)?;
    let manifest: toml::Value = contents.parse().context("Invalid release manifest TOML")?;
    if manifest
        .get("manifest_version")
        .and_then(toml::Value::as_integer)
        != Some(1)
        || manifest.get("id").and_then(toml::Value::as_str) != Some("oqto-dist")
        || manifest
            .get("release")
            .and_then(|value| value.get("target"))
            .and_then(toml::Value::as_str)
            != Some("full")
    {
        anyhow::bail!("Invalid artifact: only the declared full release can be activated");
    }

    let bin_src = release_dir.join("immutable/bin");
    if !fs::symlink_metadata(&bin_src).is_ok_and(|meta| meta.file_type().is_dir()) {
        anyhow::bail!(
            "Invalid artifact layout: missing regular directory {}",
            bin_src.display()
        );
    }
    let mut seen = std::collections::HashSet::new();
    for entry in
        fs::read_dir(&bin_src).with_context(|| format!("Failed reading {}", bin_src.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !REQUIRED_BINS.contains(&name.as_ref()) {
            anyhow::bail!(
                "Invalid artifact: unexpected bin entry {}",
                entry.path().display()
            );
        }
        let meta = fs::symlink_metadata(entry.path())?;
        if !meta.file_type().is_file() || meta.permissions().mode() & 0o111 == 0 {
            anyhow::bail!("Invalid artifact: binary must be a regular executable file: {name}");
        }
        seen.insert(name.into_owned());
    }
    for name in REQUIRED_BINS {
        if !seen.contains(name) {
            anyhow::bail!("Invalid full release: missing required binary {name}");
        }
    }
    Ok(())
}

/// Resolve the absolute target a symlink points at, or `None` if `link` is
/// absent or is not a symlink.
fn read_link_target(link: &Path) -> Option<PathBuf> {
    let meta = fs::symlink_metadata(link).ok()?;
    if !meta.file_type().is_symlink() {
        return None;
    }
    let target = fs::read_link(link).ok()?;
    if target.is_absolute() {
        Some(target)
    } else {
        link.parent().map(|parent| parent.join(target))
    }
}

fn verify_checksum(artifact: &Path, checksum_path: &Path) -> Result<()> {
    // One in-process sha256 verifier shared with the acquisition driver — no
    // dependency on an external `sha256sum` binary.
    acquire::verify_sha256(artifact, checksum_path)
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
    extract_tarball_with_limits(artifact, dst, 100_000, 2 * 1024 * 1024 * 1024)
}

fn extract_tarball_with_limits(
    artifact: &Path,
    dst: &Path,
    max_entries: usize,
    max_unpacked_bytes: u64,
) -> Result<()> {
    let root = release_id_from_artifact(artifact)?;
    let source = File::open(artifact)
        .with_context(|| format!("Failed opening release artifact {}", artifact.display()))?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(source));
    let mut seen = HashSet::new();
    let mut total_bytes = 0_u64;
    for entry in archive.entries().context("Invalid release archive")? {
        let mut entry = entry.context("Invalid release archive entry")?;
        let path = entry
            .path()
            .context("Invalid release archive path")?
            .into_owned();
        let mut components = path.components().filter(|part| *part != Component::CurDir);
        if components.next() != Some(Component::Normal(root.as_ref())) {
            anyhow::bail!("Invalid release archive: entry outside its declared root");
        }
        let mut relative = PathBuf::new();
        for component in components {
            match component {
                Component::Normal(name) => relative.push(name),
                _ => anyhow::bail!("Invalid release archive: unsafe member path"),
            }
        }
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            anyhow::bail!("Invalid release archive: links and special files are forbidden");
        }
        if relative.as_os_str().is_empty() && !kind.is_dir() {
            anyhow::bail!("Invalid release archive: root must be a directory");
        }
        if !seen.insert(relative.clone()) || seen.len() > max_entries {
            anyhow::bail!("Invalid release archive: duplicate or excessive members");
        }
        let size = entry.size();
        total_bytes = total_bytes
            .checked_add(size)
            .context("Release archive size overflow")?;
        if total_bytes > max_unpacked_bytes {
            anyhow::bail!("Invalid release archive: unpacked content exceeds limit");
        }
        let mode = entry.header().mode().context("Invalid release file mode")?;
        if mode & 0o7000 != 0 {
            anyhow::bail!("Invalid release archive: privileged file mode");
        }
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = dst.join(relative);
        if kind.is_dir() {
            if let Ok(existing) = fs::symlink_metadata(&target) {
                if !existing.file_type().is_dir() {
                    anyhow::bail!("Invalid release archive: directory collides with file");
                }
            } else {
                fs::create_dir_all(&target)?;
            }
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            let copied = io::copy(&mut entry, &mut output)?;
            if copied != size {
                anyhow::bail!("Invalid release archive: truncated file data");
            }
            fs::set_permissions(&target, fs::Permissions::from_mode(mode & 0o777))?;
        }
    }
    if seen.is_empty() {
        anyhow::bail!("Invalid release archive: no members");
    }
    // Drain bounded tar padding to force the gzip CRC/trailer to be checked.
    // `tar` stops at the end-of-archive marker, before a decoder necessarily
    // observes a truncated or corrupt gzip footer.
    let decoder = archive.into_inner();
    let padding = io::copy(&mut decoder.take(1_048_577), &mut io::sink())?;
    if padding > 1_048_576 {
        anyhow::bail!("Invalid release archive: excessive trailing content");
    }
    Ok(())
}

/// Atomically point `link` at `target` (write a sibling temp symlink, then
/// rename over `link`) so readers never observe a missing or half-written link.
fn atomic_symlink(link: &Path, target: &Path) -> Result<()> {
    let tmp_link = link.with_extension("tmp");
    // exists() follows symlinks, so a dangling temp link reports absent; check
    // symlink_metadata too and clear whatever is there before recreating.
    if fs::symlink_metadata(&tmp_link).is_ok() {
        fs::remove_file(&tmp_link).ok();
    }
    symlink(target, &tmp_link).with_context(|| {
        format!(
            "Failed to create temporary symlink {} -> {}",
            tmp_link.display(),
            target.display()
        )
    })?;
    fs::rename(&tmp_link, link).with_context(|| {
        format!(
            "Failed to atomically update {} -> {}",
            link.display(),
            target.display()
        )
    })?;
    Ok(())
}

/// Pure selection of which release directory names to prune, given each
/// candidate's modification time. Sorts newest-first, never selects the
/// `current`/`last_good` targets, keeps the `keep` newest of the remainder, and
/// returns the rest. Mirrors deploy.sh `prune_old_releases`; kept pure so the
/// retention policy is unit-testable without touching the filesystem.
fn select_prunable(
    mut releases: Vec<(String, std::time::SystemTime)>,
    current: Option<&str>,
    last_good: Option<&str>,
    keep: usize,
) -> Vec<String> {
    releases.sort_by_key(|(_, mtime)| std::cmp::Reverse(*mtime));
    let mut kept = 0usize;
    let mut prune = Vec::new();
    for (name, _) in releases {
        if Some(name.as_str()) == current || Some(name.as_str()) == last_good {
            continue;
        }
        if kept < keep {
            kept += 1;
            continue;
        }
        prune.push(name);
    }
    prune
}

/// Basename of the directory a release pointer symlink resolves to.
fn link_basename(link: &Path) -> Option<String> {
    let target = read_link_target(link)?;
    target
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string)
}

/// Remove superseded release directories under `releases_root`, always
/// preserving `current`, `last-good`, and the `keep` newest of the rest.
/// Returns the names removed.
fn prune_old_releases(releases_root: &Path, keep: usize) -> Result<Vec<String>> {
    if keep == 0 {
        return Ok(Vec::new());
    }
    let current = link_basename(&releases_root.join("current"));
    let last_good = link_basename(&releases_root.join("last-good"));

    let mut candidates = Vec::new();
    for entry in fs::read_dir(releases_root)
        .with_context(|| format!("Failed reading releases root {}", releases_root.display()))?
    {
        let entry = entry?;
        // DirEntry::file_type does not follow symlinks; skip the `current` /
        // `last-good` pointers and any non-directory, keep real release dirs.
        let file_type = entry.file_type()?;
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        let name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(_) => continue,
        };
        let mtime = entry
            .metadata()?
            .modified()
            .unwrap_or(std::time::UNIX_EPOCH);
        candidates.push((name, mtime));
    }

    let prune = select_prunable(candidates, current.as_deref(), last_good.as_deref(), keep);
    for name in &prune {
        let dir = releases_root.join(name);
        fs::remove_dir_all(&dir)
            .with_context(|| format!("Failed pruning release dir {}", dir.display()))?;
    }
    Ok(prune)
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
        match fs::symlink_metadata(&dst) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(meta) if meta.file_type().is_symlink() && fs::read_link(&dst)? == path => {
                fs::remove_file(&dst)?;
            }
            Ok(_) => anyhow::bail!("refusing to replace unmanaged entrypoint {}", dst.display()),
            Err(error) => return Err(error.into()),
        }
        symlink(&path, &dst)
            .with_context(|| format!("Failed linking {} -> {}", dst.display(), path.display()))?;
    }
    Ok(())
}

fn run_doctor_strict(release_dir: &Path) -> Result<()> {
    // Do not trust PATH (or a stale host binary) for the activation gate. The
    // staged executable is the one being promoted; a missing interpreter or
    // failed spawn must roll the transaction back, never turn strict into a
    // best-effort warning. Doctor output may contain host details or secrets.
    let staged = release_dir.join("immutable/bin/oqtoctl");
    let output = ProcessCommand::new(&staged)
        .args(["doctor", "--contract", "--profile", "auto", "--strict"])
        .output()
        .with_context(|| {
            format!(
                "Post-activation doctor could not start {}",
                staged.display()
            )
        })?;
    if !output.status.success() {
        anyhow::bail!(
            "Post-activation doctor strict failed (status: {})",
            output.status
        );
    }
    println!("Post-activation doctor strict passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn mk_release_dir(root: &Path, name: &str, binary: Option<&str>) {
        let release = root.join(name);
        let bin = release.join("immutable/bin");
        fs::create_dir_all(&bin).unwrap();
        if let Some(b) = binary {
            let binary = bin.join(b);
            fs::write(&binary, b"#!/bin/true\n").unwrap();
            fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
            if b == "oqto" {
                fs::write(
                    release.join("manifest.toml"),
                    "manifest_version = 1\nid = \"oqto-dist\"\n[release]\ntarget = \"full\"\n",
                )
                .unwrap();
                for name in [
                    "oqtoctl",
                    "oqto-setup",
                    "oqto-runner",
                    "oqto-files",
                    "oqto-sandbox",
                    "oqto-usermgr",
                    "pi-bridge",
                ] {
                    let binary = bin.join(name);
                    fs::write(&binary, b"#!/bin/true\n").unwrap();
                    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
                }
            }
        }
    }

    #[test]
    fn release_extractor_denies_excessive_entry_count_without_large_fixture() -> Result<()> {
        let root = tempfile::tempdir()?;
        let artifact = root.path().join("oqto-count-test.tar.gz");
        let encoded =
            flate2::write::GzEncoder::new(File::create(&artifact)?, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoded);
        for name in ["first", "second", "third"] {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Directory);
            header.set_size(0);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, format!("oqto-count-test/{name}"), io::empty())?;
        }
        builder.into_inner()?.finish()?;
        let stage = root.path().join("stage");
        fs::create_dir(&stage)?;
        let error = extract_tarball_with_limits(&artifact, &stage, 2, u64::MAX)
            .expect_err("third entry must exceed the configured count");
        assert!(error.to_string().contains("excessive members"));
        assert!(!stage.join("third").exists());
        Ok(())
    }

    #[test]
    fn select_prunable_keeps_newest_and_drops_rest() {
        let releases = vec![
            ("a".to_string(), at(10)),
            ("b".to_string(), at(30)),
            ("c".to_string(), at(20)),
            ("d".to_string(), at(5)),
        ];
        // Newest-first: b(30), c(20), a(10), d(5). Keep 1 -> keep b, prune c,a,d.
        let prune = select_prunable(releases, None, None, 1);
        assert_eq!(prune, vec!["c", "a", "d"]);
    }

    #[test]
    fn select_prunable_never_touches_current_or_last_good() {
        let releases = vec![
            ("a".to_string(), at(10)),
            ("b".to_string(), at(30)),
            ("c".to_string(), at(20)),
            ("d".to_string(), at(5)),
        ];
        // current=a, last_good=d are skipped entirely; remaining newest-first
        // b(30), c(20); keep 1 -> keep b, prune c.
        let prune = select_prunable(releases, Some("a"), Some("d"), 1);
        assert_eq!(prune, vec!["c"]);
    }

    #[test]
    fn prune_old_releases_disabled_when_keep_zero() {
        let root = tempfile::tempdir().unwrap();
        for name in ["r1", "r2", "r3"] {
            mk_release_dir(root.path(), name, Some("oqto"));
        }
        let removed = prune_old_releases(root.path(), 0).unwrap();
        assert!(removed.is_empty());
        for name in ["r1", "r2", "r3"] {
            assert!(root.path().join(name).is_dir());
        }
    }

    #[test]
    fn prune_old_releases_preserves_current_and_last_good() {
        let root = tempfile::tempdir().unwrap();
        for name in ["a", "b", "c", "d", "e"] {
            mk_release_dir(root.path(), name, Some("oqto"));
        }
        atomic_symlink(&root.path().join("current"), &root.path().join("a")).unwrap();
        atomic_symlink(&root.path().join("last-good"), &root.path().join("b")).unwrap();

        // keep=1: a & b preserved; of {c,d,e} keep 1 newest, prune 2.
        let removed = prune_old_releases(root.path(), 1).unwrap();
        assert_eq!(removed.len(), 2, "should prune two of c/d/e");
        assert!(root.path().join("a").is_dir(), "current target survives");
        assert!(root.path().join("b").is_dir(), "last-good target survives");
        let survivors = ["c", "d", "e"]
            .iter()
            .filter(|n| root.path().join(n).is_dir())
            .count();
        assert_eq!(survivors, 1, "exactly one of c/d/e remains");
        // The pointer symlinks themselves are untouched.
        assert_eq!(
            link_basename(&root.path().join("current")).as_deref(),
            Some("a")
        );
        assert_eq!(
            link_basename(&root.path().join("last-good")).as_deref(),
            Some("b")
        );
    }

    #[test]
    fn atomic_symlink_creates_and_repoints() {
        let root = tempfile::tempdir().unwrap();
        let link = root.path().join("current");
        let a = root.path().join("rel-a");
        let b = root.path().join("rel-b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();

        atomic_symlink(&link, &a).unwrap();
        assert_eq!(read_link_target(&link).as_deref(), Some(a.as_path()));

        // Repoint over an existing link; no leftover temp link.
        atomic_symlink(&link, &b).unwrap();
        assert_eq!(read_link_target(&link).as_deref(), Some(b.as_path()));
        assert!(!root.path().join("current.tmp").exists());
    }

    #[test]
    fn read_link_target_none_for_missing_or_plain_file() {
        let root = tempfile::tempdir().unwrap();
        assert!(read_link_target(&root.path().join("nope")).is_none());
        let plain = root.path().join("plain");
        fs::write(&plain, b"x").unwrap();
        assert!(read_link_target(&plain).is_none());
    }

    #[test]
    fn validate_staged_release_requires_a_binary() {
        let root = tempfile::tempdir().unwrap();

        // Missing immutable/bin entirely.
        let empty = root.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert!(validate_staged_release(&empty).is_err());

        // immutable/bin exists but has no files.
        mk_release_dir(root.path(), "nobins", None);
        assert!(validate_staged_release(&root.path().join("nobins")).is_err());

        // Valid layout with a staged binary.
        mk_release_dir(root.path(), "ok", Some("oqto"));
        assert!(validate_staged_release(&root.path().join("ok")).is_ok());
    }

    #[test]
    fn validate_staged_release_rejects_deferred_doctor_runner_target_and_missing_assets() {
        let root = tempfile::tempdir().unwrap();
        mk_release_dir(root.path(), "candidate", Some("oqto"));
        let candidate = root.path().join("candidate");
        fs::write(
            candidate.join("manifest.toml"),
            "manifest_version = 1\nid = \"oqto-dist\"\n[release]\ntarget = \"runner_only\"\n",
        )
        .unwrap();
        assert!(validate_staged_release(&candidate).is_err());
        fs::write(
            candidate.join("manifest.toml"),
            "manifest_version = 1\nid = \"oqto-dist\"\n[release]\ntarget = \"full\"\n",
        )
        .unwrap();
        fs::remove_file(candidate.join("immutable/bin/pi-bridge")).unwrap();
        assert!(validate_staged_release(&candidate).is_err());
    }

    #[test]
    fn validate_staged_release_rejects_binary_symlinks() {
        let root = tempfile::tempdir().unwrap();
        mk_release_dir(root.path(), "candidate", Some("oqto"));
        let candidate = root.path().join("candidate");
        fs::remove_file(candidate.join("immutable/bin/pi-bridge")).unwrap();
        symlink("/usr/bin/true", candidate.join("immutable/bin/pi-bridge")).unwrap();
        assert!(validate_staged_release(&candidate).is_err());
    }

    #[test]
    fn validate_staged_release_rejects_non_executable_binary() {
        let root = tempfile::tempdir().unwrap();
        mk_release_dir(root.path(), "candidate", Some("oqto"));
        let candidate = root.path().join("candidate");
        let binary = candidate.join("immutable/bin/pi-bridge");
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(validate_staged_release(&candidate).is_err());
    }

    #[test]
    fn validate_staged_release_rejects_hidden_bin_placeholders() {
        let root = tempfile::tempdir().unwrap();
        mk_release_dir(root.path(), "placeholder", Some(".gitkeep"));
        assert!(validate_staged_release(&root.path().join("placeholder")).is_err());
    }

    #[test]
    fn relink_bins_symlinks_each_binary() {
        let root = tempfile::tempdir().unwrap();
        let bin_src = root.path().join("rel/immutable/bin");
        fs::create_dir_all(&bin_src).unwrap();
        for b in ["oqto", "oqto-runner"] {
            fs::write(bin_src.join(b), b"#!/bin/true\n").unwrap();
        }
        let bin_dir = root.path().join("usr-local-bin");

        relink_bins(&bin_src, &bin_dir).unwrap();
        for b in ["oqto", "oqto-runner"] {
            let link = bin_dir.join(b);
            assert!(read_link_target(&link).is_some(), "{b} should be a symlink");
            assert_eq!(
                read_link_target(&link).as_deref(),
                Some(bin_src.join(b).as_path())
            );
        }
    }

    #[test]
    fn relink_bins_never_removes_an_unmanaged_directory() {
        let root = tempfile::tempdir().unwrap();
        let bin_src = root.path().join("rel/immutable/bin");
        fs::create_dir_all(&bin_src).unwrap();
        fs::write(bin_src.join("oqto"), b"release executable").unwrap();
        let bin_dir = root.path().join("bin");
        let unmanaged = bin_dir.join("oqto");
        fs::create_dir_all(&unmanaged).unwrap();
        fs::write(unmanaged.join("user-data"), b"preserve").unwrap();

        assert!(relink_bins(&bin_src, &bin_dir).is_err());
        assert_eq!(fs::read(unmanaged.join("user-data")).unwrap(), b"preserve");
    }

    #[test]
    fn release_id_from_artifact_strips_tar_gz() {
        let id = release_id_from_artifact(Path::new("/tmp/oqto-0.4.0-x86_64.tar.gz")).unwrap();
        assert_eq!(id, "oqto-0.4.0-x86_64");
    }

    /// Stage a release dir directly (no tar subprocess) and activate it; assert
    /// the full transaction lands `current`, `last-good`, and relinked bins.
    #[test]
    fn activate_release_marks_current_last_good_and_relinks() {
        let root = tempfile::tempdir().unwrap();
        let releases_root = root.path().join("releases");
        let bin_dir = root.path().join("bin");
        mk_release_dir(&releases_root, "oqto-9.9.9-test", Some("oqto"));
        let release_dir = releases_root.join("oqto-9.9.9-test");

        // doctor_strict=false so we don't depend on oqtoctl being installed.
        activate_release(&release_dir, &releases_root, &bin_dir, false, 3).unwrap();

        assert_eq!(
            link_basename(&releases_root.join("current")).as_deref(),
            Some("oqto-9.9.9-test")
        );
        assert_eq!(
            link_basename(&releases_root.join("last-good")).as_deref(),
            Some("oqto-9.9.9-test")
        );
        // Entrypoints relink through `current` (ADR-0016), so they auto-follow
        // future switches without re-linking.
        assert_eq!(
            read_link_target(&bin_dir.join("oqto")).as_deref(),
            Some(releases_root.join("current/immutable/bin/oqto").as_path()),
            "oqto should be relinked through current"
        );
    }

    /// A second activation supersedes the first: `current`/`last-good`/bins all
    /// advance to the newer release.
    #[test]
    fn activate_release_supersedes_previous_current() {
        let root = tempfile::tempdir().unwrap();
        let releases_root = root.path().join("releases");
        let bin_dir = root.path().join("bin");
        mk_release_dir(&releases_root, "rel-old", Some("oqto"));
        mk_release_dir(&releases_root, "rel-new", Some("oqto"));

        activate_release(
            &releases_root.join("rel-old"),
            &releases_root,
            &bin_dir,
            false,
            3,
        )
        .unwrap();
        activate_release(
            &releases_root.join("rel-new"),
            &releases_root,
            &bin_dir,
            false,
            3,
        )
        .unwrap();

        assert_eq!(
            link_basename(&releases_root.join("current")).as_deref(),
            Some("rel-new")
        );
        assert_eq!(
            link_basename(&releases_root.join("last-good")).as_deref(),
            Some("rel-new")
        );
        assert_eq!(
            read_link_target(&bin_dir.join("oqto")).as_deref(),
            Some(releases_root.join("current/immutable/bin/oqto").as_path())
        );
    }

    /// Activation aborts (and does not advance last-good) when the staged layout
    /// is invalid — fail-closed before touching `current`.
    #[test]
    fn activate_release_rejects_invalid_layout() {
        let root = tempfile::tempdir().unwrap();
        let releases_root = root.path().join("releases");
        let bin_dir = root.path().join("bin");
        // immutable/bin exists but is empty -> no binaries to relink.
        mk_release_dir(&releases_root, "rel-empty", None);

        let result = activate_release(
            &releases_root.join("rel-empty"),
            &releases_root,
            &bin_dir,
            false,
            3,
        );
        assert!(result.is_err(), "empty release must be rejected");
        assert!(
            read_link_target(&releases_root.join("current")).is_none(),
            "current must not be switched to an invalid release"
        );
        assert!(read_link_target(&releases_root.join("last-good")).is_none());
    }
}
