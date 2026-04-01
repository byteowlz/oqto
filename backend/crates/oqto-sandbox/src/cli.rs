use anyhow::{Context, Result};
use clap::Parser;
use log::{debug, error, info};
use std::path::PathBuf;
use std::process::Command;

#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;

use crate::{SandboxConfig, SandboxConfigFile};

#[derive(Parser, Debug)]
#[command(
    name = "oqto-sandbox",
    about = "Sandbox wrapper for agent processes",
    after_help = "Examples:\n  \
        oqto-sandbox --profile development -- agent serve\n  \
        oqto-sandbox --config ./sandbox.toml -- cargo build\n  \
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

    #[arg(last = true, required = true)]
    command: Vec<String>,
}

fn load_config(args: &Args) -> Result<SandboxConfig> {
    if let Some(config_path) = &args.config {
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("reading config file: {:?}", config_path))?;
        let file: SandboxConfigFile =
            toml::from_str(&content).with_context(|| "parsing config file")?;
        let mut config: SandboxConfig = file.into();
        config.enabled = !args.no_sandbox;
        Ok(config)
    } else {
        let mut config = SandboxConfig::from_profile(&args.profile);
        config.enabled = !args.no_sandbox;
        Ok(config)
    }
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

    let seccomp_file = config.open_seccomp_bpf_file(None)?;
    let seccomp_fd = seccomp_file.as_ref().map(AsRawFd::as_raw_fd);
    let no_new_privs = config.no_new_privs;
    let landlock_cfg = config.clone();
    let landlock_workspace = workspace.to_path_buf();

    if seccomp_fd.is_some() || no_new_privs || landlock_cfg.landlock_mode != crate::LandlockMode::Off {
        // Keep file alive until exec by moving into closure.
        let _keep_alive = seccomp_file;
        // SAFETY: pre_exec runs in child after fork, before exec.
        unsafe {
            cmd.pre_exec(move || {
                if no_new_privs {
                    let rc = libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
                    if rc != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }

                landlock_cfg.apply_landlock(&landlock_workspace, None)?;

                if let Some(fd) = seccomp_fd {
                    if libc::dup2(fd, 3) == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
    }

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
