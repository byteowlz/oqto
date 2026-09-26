use anyhow::{Context, Result};
use clap::Parser;
use log::{debug, error, info};
#[cfg(target_os = "macos")]
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[cfg(target_os = "linux")]
use crate::configure_bwrap_pre_exec;
use crate::landlock_shim::maybe_run_shim;
use crate::{SandboxConfig, SandboxConfigFile};

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

#[cfg(target_os = "macos")]
fn exec_sandboxed(
    config: &SandboxConfig,
    command: &[String],
    workspace: &Path,
    dry_run: bool,
) -> Result<()> {
    fn checked_policy_path(path: &str) -> Result<&str> {
        // Paths are interpolated into quoted Seatbelt DSL strings. Until there
        // is a tested platform encoder, reject characters that can terminate
        // a literal or introduce another rule rather than broadening grants.
        if path.is_empty()
            || path.chars().any(|c| {
                c == '"' || c == '\\' || c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
            })
        {
            anyhow::bail!("Seatbelt policy path contains unsafe characters");
        }
        Ok(path)
    }

    fn build_seatbelt_profile(config: &SandboxConfig, workspace: &Path) -> Result<String> {
        let workspace = workspace
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Seatbelt policy workspace is not UTF-8"))?;
        let w = checked_policy_path(workspace)?;
        for path in config
            .allow_write
            .iter()
            .chain(&config.deny_read)
            .chain(&config.deny_write)
        {
            checked_policy_path(path)?;
        }
        let mut profile = String::new();
        profile.push_str("(version 1)\n");
        profile.push_str("(deny default)\n");
        profile.push_str("(allow process-fork)\n");
        profile.push_str("(allow process-exec)\n");
        profile.push_str("(allow signal)\n");
        profile.push_str("(allow file-read*)\n");

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

        Ok(profile)
    }

    fn build_sandbox_exec_args(
        config: &SandboxConfig,
        workspace: &Path,
    ) -> Result<Option<(Vec<String>, tempfile::NamedTempFile)>> {
        let profile_text = build_seatbelt_profile(config, workspace)?;
        if which::which("sandbox-exec").is_err() {
            return Ok(None);
        }
        let mut tmp = tempfile::NamedTempFile::new()?;
        use std::io::Write;
        tmp.write_all(profile_text.as_bytes())?;
        let args = vec!["-f".to_string(), tmp.path().to_string_lossy().to_string()];
        Ok(Some((args, tmp)))
    }

    let (sandbox_args, _temp_file) = match build_sandbox_exec_args(config, workspace)? {
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
        println!("{}", build_seatbelt_profile(config, workspace)?);
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

#[cfg(all(test, target_os = "macos"))]
mod seatbelt_policy_tests {
    use super::*;

    #[test]
    fn rejects_workspace_policy_injection_before_any_launch() {
        let config = SandboxConfig::from_profile("minimal");
        let command = vec!["/bin/false".to_string()];
        for path in [
            "/tmp/ws\") (allow file-write* (subpath \"/\"))",
            "/tmp/ws\\bad",
            "/tmp/ws\n(allow network*)",
        ] {
            let error = exec_sandboxed(&config, &command, Path::new(path), false)
                .expect_err("unsafe workspace path must fail closed");
            assert!(error.to_string().contains("unsafe characters"));
        }
    }

    #[test]
    fn rejects_unsafe_grants_but_accepts_spaced_unicode_paths() -> anyhow::Result<()> {
        let command = vec!["/bin/false".to_string()];
        let workspace = Path::new("/tmp/Oqto workdir ü");
        for grant in [
            "/tmp/grant\") (allow file-write*)",
            "/tmp/grant\\escape",
            "/tmp/grant\n(allow network*)",
        ] {
            let mut config = SandboxConfig::from_profile("minimal");
            config.allow_write = vec![grant.to_string()];
            let error = exec_sandboxed(&config, &command, workspace, false)
                .expect_err("unsafe allow_write path must fail closed");
            assert!(error.to_string().contains("unsafe characters"));
        }
        let config = SandboxConfig::from_profile("minimal");
        exec_sandboxed(&config, &command, workspace, true)?;
        Ok(())
    }
}
