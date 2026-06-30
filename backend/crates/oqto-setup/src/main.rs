use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
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
            checksum.as_deref(),
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
        } => acquire_bundle(&manifest, arch, &base_url, &dest, install_bin.as_deref()),
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
) -> Result<()> {
    let contents = fs::read_to_string(manifest)
        .with_context(|| format!("Failed to read dependency manifest: {}", manifest.display()))?;
    let components = deps::parse_dependency_manifest(&contents)?;
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
            .filter(|(c, _)| !matches!(c.naming, deps::ArtifactNaming::Oqto))
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
#[allow(clippy::too_many_arguments)]
fn install_release(
    artifact: &Path,
    checksum: Option<&Path>,
    releases_root: &Path,
    bin_dir: &Path,
    doctor_strict: bool,
    keep_releases: usize,
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

    activate_release(
        &release_dir,
        releases_root,
        bin_dir,
        doctor_strict,
        keep_releases,
    )?;

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

    atomic_symlink(&current_link, release_dir)?;
    relink_bins(&current_link.join("immutable/bin"), bin_dir)?;

    if doctor_strict && let Err(doctor_err) = run_doctor_strict() {
        match previous.as_ref() {
            Some(prev) => {
                atomic_symlink(&current_link, prev)?;
                // Relink through `current` (now pointing at prev) so the stable
                // entrypoints stay consistent with the activation path.
                relink_bins(&current_link.join("immutable/bin"), bin_dir)?;
                anyhow::bail!(
                    "Activation failed ({doctor_err}); rolled back to {}",
                    prev.display()
                );
            }
            None => anyhow::bail!(
                "Activation failed ({doctor_err}); no previous release to roll back to"
            ),
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

/// Reject an extracted release whose layout cannot be activated (no shipped
/// binaries to relink). Cheap fail-closed gate before we touch `current`.
fn validate_staged_release(release_dir: &Path) -> Result<()> {
    let bin_src = release_dir.join("immutable/bin");
    if !bin_src.is_dir() {
        anyhow::bail!("Invalid artifact layout: missing {}", bin_src.display());
    }
    let has_binary = fs::read_dir(&bin_src)
        .with_context(|| format!("Failed reading {}", bin_src.display()))?
        .filter_map(|entry| entry.ok())
        .any(|entry| entry.path().is_file());
    if !has_binary {
        anyhow::bail!(
            "Invalid artifact: no binaries staged in {}",
            bin_src.display()
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn mk_release_dir(root: &Path, name: &str, binary: Option<&str>) {
        let bin = root.join(name).join("immutable/bin");
        fs::create_dir_all(&bin).unwrap();
        if let Some(b) = binary {
            fs::write(bin.join(b), b"#!/bin/true\n").unwrap();
        }
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
