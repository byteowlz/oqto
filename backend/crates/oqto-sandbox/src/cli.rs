use anyhow::{Context, Result};
use clap::Parser;
use log::{debug, error, info};
#[cfg(target_os = "macos")]
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use crate::landlock_shim::maybe_run_shim;
use crate::{SandboxConfig, SandboxConfigFile, configure_bwrap_pre_exec};

#[derive(Parser, Debug)]
#[command(
    name = "oqto-sandbox",
    about = "Sandbox wrapper for agent processes",
    trailing_var_arg = true,
    after_help = "Examples:\n  \
        oqto-sandbox ls -la\n  \
        oqto-sandbox --profile development -- agent serve\n  \
        oqto-sandbox --config ./sandbox.toml cargo build\n  \
        oqto-sandbox --dry-run --profile strict -- npm install"
)]
struct Args {
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Profile to use. Overrides the profile named by any config file; when
    /// omitted the config file's own profile (or "development") is used.
    #[arg(short, long)]
    profile: Option<String>,

    #[arg(short, long)]
    workspace: Option<PathBuf>,

    #[arg(long)]
    dry_run: bool,

    #[arg(long)]
    no_sandbox: bool,

    #[arg(short, long)]
    verbose: bool,

    #[arg(trailing_var_arg = true, required = true)]
    command: Vec<String>,
}

/// Config lookup chain (first match wins):
/// 1. Explicit `--config` flag
/// 2. `/etc/oqto/sandbox.toml` (system, trusted)
/// 3. `~/.config/oqto/sandbox.toml` (user)
/// 4. Hardcoded profile defaults
///
/// Workspace config (`.oqto/sandbox.toml`) is merged on top in `run_cli`
/// and can only add restrictions, never weaken them.
fn load_config(args: &Args) -> Result<SandboxConfig> {
    let mut config = if let Some(config_path) = &args.config {
        info!(
            "Loading sandbox config from explicit path: {:?}",
            config_path
        );
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("reading config file: {:?}", config_path))?;
        let mut file: SandboxConfigFile =
            toml::from_str(&content).with_context(|| "parsing config file")?;
        if let Some(profile) = &args.profile {
            profile.clone_into(&mut file.profile);
        }
        file.into()
    } else {
        load_config_from_chain(args.profile.as_deref())?
    };

    config.enabled = !args.no_sandbox;
    Ok(config)
}

const SYSTEM_SANDBOX_CONFIG: &str = "/etc/oqto/sandbox.toml";

/// Load config from the standard lookup chain:
/// 1. `/etc/oqto/sandbox.toml` (system)
/// 2. `~/.config/oqto/sandbox.toml` (user)
/// 3. Hardcoded profile defaults
fn load_config_from_chain(profile: Option<&str>) -> Result<SandboxConfig> {
    let system_path = PathBuf::from(SYSTEM_SANDBOX_CONFIG);
    if system_path.exists() {
        info!("Loading sandbox config from system path: {:?}", system_path);
        let content = std::fs::read_to_string(&system_path)
            .with_context(|| format!("reading system config: {:?}", system_path))?;
        let mut file: SandboxConfigFile = toml::from_str(&content)
            .with_context(|| format!("parsing system config: {:?}", system_path))?;
        // An explicit --profile must win over the file's profile, otherwise a
        // system config silently downgrades the caller's requested isolation.
        if let Some(profile) = profile {
            info!("Overriding system config profile with '{}'", profile);
            profile.clone_into(&mut file.profile);
        }
        return Ok(file.into());
    }

    let user_path = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("oqto")
        .join("sandbox.toml");

    if user_path.exists() {
        info!("Loading sandbox config from user path: {:?}", user_path);
        return SandboxConfig::load_user_config()
            .context("user sandbox config exists but failed to parse");
    }

    let profile = profile.unwrap_or("development");
    info!(
        "No config file found, using hardcoded profile '{}'",
        profile
    );
    Ok(SandboxConfig::from_profile(profile))
}

#[cfg(unix)]
fn exec_direct(command: &[String], workspace: &PathBuf) -> Result<()> {
    let err = Command::new(&command[0])
        .args(&command[1..])
        .current_dir(workspace)
        .exec();
    error!("Failed to exec: {:?}", err);
    Err(err.into())
}

#[cfg(target_os = "linux")]
fn exec_sandboxed(
    config: &SandboxConfig,
    command: &[String],
    workspace: &std::path::Path,
    dry_run: bool,
) -> Result<()> {
    let bwrap_args = match config.build_bwrap_args_for_user(workspace, None) {
        Some(args) => args,
        None => {
            // build_bwrap_args_for_user returns None for several reasons:
            // - bwrap binary missing
            // - seccomp_mode=enforce with missing/unreadable bpf
            // - landlock_mode=enforce without kernel support or shim binary
            // Check each cause so the user sees the right error and dry-run
            // returns a non-zero exit code instead of a silent success.
            let msg = if !SandboxConfig::is_bwrap_available() {
                "bubblewrap (bwrap) not found in PATH"
            } else {
                "sandbox config rejected (seccomp/landlock enforce without backing support — see log above)"
            };
            error!("{}", msg);
            if dry_run {
                println!("ERROR: {}", msg);
            }
            anyhow::bail!("{}", msg);
        }
    };

    let mut full_args = bwrap_args;
    full_args.extend(command.iter().cloned());

    if dry_run {
        println!("bwrap {}", full_args.join(" \\\n  "));
        return Ok(());
    }

    // Set up network egress (proxy mode creates a namespace; open/isolated are
    // inert). The guard tears the namespace down on drop, so it must outlive the
    // sandboxed process -- which means we cannot `exec()` it away in proxy mode.
    let egress = config.prepare_egress()?;

    debug!("Executing: bwrap {:?}", full_args);
    let mut cmd = Command::new("bwrap");
    cmd.args(&full_args);

    configure_bwrap_pre_exec(&mut cmd, config, workspace, egress.plan())?;

    if egress.plan().is_some() {
        // Proxy mode: supervise rather than exec, so the egress guard's Drop
        // runs teardown after the child exits. Mirror the child's exit code.
        let status = cmd
            .status()
            .map_err(|e| anyhow::anyhow!("failed to spawn bwrap: {e}"))?;
        drop(egress); // tear down the egress namespace before exiting
        std::process::exit(status.code().unwrap_or(1));
    }

    // No egress namespace to clean up: exec-replace as before.
    let err = cmd.exec();

    error!("Failed to exec bwrap: {:?}", err);
    Err(err.into())
}

#[cfg(all(target_os = "macos", feature = "macos-seatbelt"))]
fn exec_sandboxed(
    config: &SandboxConfig,
    command: &[String],
    workspace: &Path,
    dry_run: bool,
) -> Result<()> {
    use std::io::Write;

    let profile_text = crate::seatbelt::compile_profile(config, workspace, None);

    if dry_run {
        println!("sandbox-exec -f <profile> {}", command.join(" "));
        println!("\n# Seatbelt profile:");
        println!("{profile_text}");
        return Ok(());
    }

    if which::which("sandbox-exec").is_err() {
        // Fail closed: running unsandboxed after being asked to sandbox would
        // silently drop every restriction the profile describes.
        anyhow::bail!("sandbox-exec not available, refusing to run unsandboxed");
    }

    let mut profile_file =
        tempfile::NamedTempFile::new().context("creating Seatbelt profile file")?;
    profile_file
        .write_all(profile_text.as_bytes())
        .context("writing Seatbelt profile")?;
    profile_file.flush().context("flushing Seatbelt profile")?;

    let mut full_args = vec![
        "-f".to_string(),
        profile_file.path().to_string_lossy().to_string(),
    ];
    full_args.extend(command.iter().cloned());

    debug!("Executing: sandbox-exec {:?}", full_args);
    let mut cmd = Command::new("sandbox-exec");
    cmd.args(&full_args);
    // sandbox-exec inherits this process's environment, so a sparse PATH from a
    // launchd job or non-interactive ssh session leaves shebang interpreters
    // (`/usr/bin/env node`) unresolvable inside the sandbox.
    cmd.env(
        "PATH",
        SandboxConfig::sandbox_path(dirs::home_dir().as_deref()),
    );
    configure_bwrap_pre_exec(&mut cmd, config, workspace, None)?;

    // exec replaces this process, so the temp profile would be unlinked before
    // sandbox-exec reads it. Keep the file alive by supervising the child and
    // mirroring its exit status instead.
    let status = cmd.status().context("spawning sandbox-exec")?;
    drop(profile_file);
    std::process::exit(status.code().unwrap_or(1));
}

/// macOS without the Seatbelt backend compiled in. Fail closed: running
/// unsandboxed after being asked to sandbox would silently drop every
/// restriction the profile describes.
#[cfg(all(target_os = "macos", not(feature = "macos-seatbelt")))]
fn exec_sandboxed(
    _config: &SandboxConfig,
    command: &[String],
    _workspace: &Path,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        println!("ERROR: built without the macos-seatbelt feature");
        println!("Would refuse to execute: {command:?}");
        return Ok(());
    }
    anyhow::bail!(
        "sandboxing requested but this build lacks the macos-seatbelt feature; \
         rebuild with --features macos-seatbelt"
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn exec_sandboxed(
    _config: &SandboxConfig,
    command: &[String],
    _workspace: &PathBuf,
    dry_run: bool,
) -> Result<()> {
    error!("Sandboxing not supported on this platform");
    if dry_run {
        println!("ERROR: Sandboxing not supported on this platform");
        println!("Would execute directly: {:?}", command);
        return Ok(());
    }
    anyhow::bail!("Sandboxing not supported on this platform")
}

pub fn run_cli() -> Result<()> {
    // Inner-shim fast path: when invoked by bwrap as its inner command with
    // OQTO_SANDBOX_SHIM_MODE=1, apply Landlock and exec the real target.
    // maybe_run_shim returns without side effects when the sentinel is absent.
    maybe_run_shim()?;

    let args = Args::parse();

    let log_level = if args.verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();

    let config = load_config(&args)?;

    let workspace = args
        .workspace
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    // Merge workspace config on top (can only add restrictions, never weaken)
    let config = config.with_workspace_config(&workspace);

    info!(
        "oqto-sandbox: platform={}, profile={}, workspace={:?}, command={:?}",
        std::env::consts::OS,
        config.profile,
        workspace,
        args.command
    );

    if args.no_sandbox || !config.enabled {
        info!("Sandbox disabled, executing directly");
        if args.dry_run {
            println!("Would execute: {:?}", args.command);
            return Ok(());
        }
        return exec_direct(&args.command, &workspace);
    }

    exec_sandboxed(&config, &args.command, &workspace, args.dry_run)
}
