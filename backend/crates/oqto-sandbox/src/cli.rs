use anyhow::{Context, Result};
use clap::Parser;
use log::{debug, error, info};
use std::path::PathBuf;
use std::process::Command;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

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

    #[arg(short, long, default_value = "development")]
    profile: String,

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
        let file: SandboxConfigFile =
            toml::from_str(&content).with_context(|| "parsing config file")?;
        file.into()
    } else {
        load_config_from_chain(&args.profile)?
    };

    config.enabled = !args.no_sandbox;
    Ok(config)
}

const SYSTEM_SANDBOX_CONFIG: &str = "/etc/oqto/sandbox.toml";

/// Load config from the standard lookup chain:
/// 1. `/etc/oqto/sandbox.toml` (system)
/// 2. `~/.config/oqto/sandbox.toml` (user)
/// 3. Hardcoded profile defaults
fn load_config_from_chain(profile: &str) -> Result<SandboxConfig> {
    let system_path = PathBuf::from(SYSTEM_SANDBOX_CONFIG);
    if system_path.exists() {
        info!("Loading sandbox config from system path: {:?}", system_path);
        let content = std::fs::read_to_string(&system_path)
            .with_context(|| format!("reading system config: {:?}", system_path))?;
        let file: SandboxConfigFile = toml::from_str(&content)
            .with_context(|| format!("parsing system config: {:?}", system_path))?;
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
            error!("bubblewrap (bwrap) not available, cannot sandbox");
            if dry_run {
                println!("ERROR: bwrap not available");
                return Ok(());
            }
            anyhow::bail!("bubblewrap (bwrap) not found in PATH");
        }
    };

    let mut full_args = bwrap_args;
    full_args.extend(command.iter().cloned());

    if dry_run {
        println!("bwrap {}", full_args.join(" \\\n  "));
        return Ok(());
    }

    debug!("Executing: bwrap {:?}", full_args);
    let mut cmd = Command::new("bwrap");
    cmd.args(&full_args);

    configure_bwrap_pre_exec(&mut cmd, config, workspace)?;

    let err = cmd.exec();

    error!("Failed to exec bwrap: {:?}", err);
    Err(err.into())
}

#[cfg(target_os = "macos")]
fn exec_sandboxed(
    config: &SandboxConfig,
    command: &[String],
    workspace: &PathBuf,
    dry_run: bool,
) -> Result<()> {
    fn build_seatbelt_profile(config: &SandboxConfig, workspace: &PathBuf) -> String {
        let mut profile = String::new();
        profile.push_str("(version 1)\n");
        profile.push_str("(deny default)\n");
        profile.push_str("(allow process-fork)\n");
        profile.push_str("(allow process-exec)\n");
        profile.push_str("(allow signal)\n");
        profile.push_str("(allow file-read*)\n");

        let w = workspace.to_string_lossy();
        profile.push_str(&format!("(allow file-read* (subpath \"{}\"))\n", w));
        profile.push_str(&format!("(allow file-write* (subpath \"{}\"))\n", w));

        for path in &config.allow_write {
            profile.push_str(&format!("(allow file-write* (subpath \"{}\"))\n", path));
        }
        for path in &config.deny_read {
            profile.push_str(&format!("(deny file-read* (subpath \"{}\"))\n", path));
            profile.push_str(&format!("(deny file-write* (subpath \"{}\"))\n", path));
        }
        for path in &config.deny_write {
            profile.push_str(&format!("(deny file-write* (subpath \"{}\"))\n", path));
        }

        if config.isolate_network {
            profile.push_str("(deny network*)\n");
        } else {
            profile.push_str("(allow network*)\n");
        }

        profile
    }

    fn build_sandbox_exec_args(
        config: &SandboxConfig,
        workspace: &PathBuf,
    ) -> Option<(Vec<String>, tempfile::NamedTempFile)> {
        if which::which("sandbox-exec").is_err() {
            return None;
        }
        let profile_text = build_seatbelt_profile(config, workspace);
        let mut tmp = tempfile::NamedTempFile::new().ok()?;
        use std::io::Write;
        tmp.write_all(profile_text.as_bytes()).ok()?;
        let args = vec!["-f".to_string(), tmp.path().to_string_lossy().to_string()];
        Some((args, tmp))
    }

    let (sandbox_args, _temp_file) = match build_sandbox_exec_args(config, workspace) {
        Some(result) => result,
        None => {
            error!("sandbox-exec not available, cannot sandbox");
            if dry_run {
                println!("ERROR: sandbox-exec not available");
                return Ok(());
            }
            anyhow::bail!("sandbox-exec not available");
        }
    };

    let mut full_args = sandbox_args;
    full_args.extend(command.iter().cloned());

    if dry_run {
        println!("sandbox-exec {}", full_args.join(" \\\n  "));
        println!("\n# Seatbelt profile:");
        println!("{}", build_seatbelt_profile(config, workspace));
        return Ok(());
    }

    debug!("Executing: sandbox-exec {:?}", full_args);
    let err = Command::new("sandbox-exec").args(&full_args).exec();

    error!("Failed to exec sandbox-exec: {:?}", err);
    Err(err.into())
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
