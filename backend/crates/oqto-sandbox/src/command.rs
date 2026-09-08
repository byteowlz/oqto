//! Platform selection for sandboxed commands, shared by CLI and runners.
//! This builds a command only: callers retain egress guards and install the
//! pre-exec hooks immediately before spawn. No unsandboxed fallback exists.

use std::path::Path;
use std::process::Command;

use crate::SandboxConfig;
use anyhow::{Context, Result, ensure};

/// Whether the payload actually inherits the caller's terminal. A runner with
/// a terminal of its own must still select Redirected for its RPC children.
#[derive(Clone, Copy, Debug)]
pub enum SandboxStdin {
    Inherited,
    Redirected,
}

pub fn build_sandbox_command(
    config: &SandboxConfig,
    workspace: &Path,
    program: &Path,
    args: &[String],
    stdin: SandboxStdin,
) -> Result<Command> {
    ensure!(
        config.enabled,
        "sandbox command requested with sandbox disabled"
    );
    let workspace = workspace
        .canonicalize()
        .context("resolving sandbox work directory")?;
    ensure!(
        workspace.is_dir(),
        "sandbox work directory is not a directory"
    );
    platform_command(config, &workspace, program, args, stdin)
}

#[cfg(target_os = "linux")]
fn platform_command(
    config: &SandboxConfig,
    workspace: &Path,
    program: &Path,
    args: &[String],
    _stdin: SandboxStdin,
) -> Result<Command> {
    let sandbox_args = config.build_bwrap_args_for_user(workspace, None).context(
        "bubblewrap unavailable or sandbox policy rejected; refusing unsandboxed execution",
    )?;
    let mut command = Command::new("bwrap");
    command.args(sandbox_args).arg(program).args(args);
    Ok(command)
}

#[cfg(all(target_os = "macos", feature = "macos-seatbelt"))]
fn platform_command(
    config: &SandboxConfig,
    workspace: &Path,
    program: &Path,
    args: &[String],
    stdin: SandboxStdin,
) -> Result<Command> {
    let mut profile = crate::seatbelt::compile_profile(config, workspace)?;
    // Native TUI needs tcsetattr on its inherited terminal. Grant ioctl only
    // for that terminal, not every device on the host. RPC has no such grant.
    use std::io::IsTerminal;
    if matches!(stdin, SandboxStdin::Inherited) && std::io::stdin().is_terminal() {
        let mut name = [0u8; 1024];
        // SAFETY: ttyname_r writes at most name.len() bytes to our live buffer.
        let status =
            unsafe { libc::ttyname_r(libc::STDIN_FILENO, name.as_mut_ptr().cast(), name.len()) };
        ensure!(
            status == 0,
            "cannot resolve inherited terminal: {}",
            std::io::Error::from_raw_os_error(status)
        );
        let terminal = std::ffi::CStr::from_bytes_until_nul(&name)
            .context("terminal path lacks NUL terminator")?
            .to_str()
            .context("terminal path is not UTF-8")?;
        let escaped = terminal.replace('\\', "\\\\").replace('"', "\\\"");
        profile.push_str(&format!("(allow file-ioctl (literal \"{escaped}\"))\n"));
    }
    let executable = which::which("sandbox-exec")
        .context("sandbox-exec unavailable; refusing unsandboxed execution")?;
    let mut command = Command::new(executable);
    // Inline profile avoids a temp-file lifetime race with asynchronous spawn.
    // Oversized profiles fail at exec rather than weakening the policy.
    command
        .arg("-p")
        .arg(profile)
        .arg(program)
        .args(args)
        .current_dir(workspace);
    command.env(
        "PATH",
        SandboxConfig::sandbox_path(dirs::home_dir().as_deref()),
    );
    if let Some(cache) = config.workspace_cache_dir(workspace, None)? {
        for (name, subdir) in crate::config::WORKSPACE_CACHE_ENV {
            command.env(name, cache.join(subdir));
        }
    }
    Ok(command)
}

#[cfg(all(target_os = "macos", not(feature = "macos-seatbelt")))]
fn platform_command(
    _: &SandboxConfig,
    _: &Path,
    _: &Path,
    _: &[String],
    _: SandboxStdin,
) -> Result<Command> {
    anyhow::bail!(
        "sandboxing requested but this build lacks macos-seatbelt; refusing unsandboxed execution"
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn platform_command(
    _: &SandboxConfig,
    _: &Path,
    _: &Path,
    _: &[String],
    _: SandboxStdin,
) -> Result<Command> {
    anyhow::bail!("sandboxing is unsupported on this platform")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_turns_disabled_policy_into_direct_execution() {
        let mut config = SandboxConfig::from_profile("minimal");
        config.enabled = false;
        assert!(
            build_sandbox_command(
                &config,
                Path::new("/"),
                Path::new("/bin/true"),
                &[],
                SandboxStdin::Redirected
            )
            .is_err()
        );
    }

    #[test]
    fn nonexistent_workspace_is_an_error() {
        let config = SandboxConfig::from_profile("minimal");
        let temp = tempfile::tempdir().unwrap();
        assert!(
            build_sandbox_command(
                &config,
                &temp.path().join("missing"),
                Path::new("/bin/true"),
                &[],
                SandboxStdin::Redirected,
            )
            .is_err()
        );
    }

    #[cfg(all(target_os = "macos", not(feature = "macos-seatbelt")))]
    #[test]
    fn missing_seatbelt_feature_fails_closed() {
        let config = SandboxConfig::from_profile("minimal");
        let temp = tempfile::tempdir().unwrap();
        let error = build_sandbox_command(
            &config,
            temp.path(),
            Path::new("/bin/true"),
            &[],
            SandboxStdin::Redirected,
        )
        .unwrap_err();
        assert!(error.to_string().contains("lacks macos-seatbelt"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_keeps_bwrap_argv_and_literal_payload_arguments() {
        if !SandboxConfig::is_bwrap_available() {
            return;
        }
        let config = SandboxConfig::from_profile("minimal");
        let temp = tempfile::tempdir().unwrap();
        let payload = vec!["space ; $(not-a-shell)".to_string()];
        let command = build_sandbox_command(
            &config,
            temp.path(),
            Path::new("/bin/echo"),
            &payload,
            SandboxStdin::Redirected,
        )
        .unwrap();
        assert_eq!(command.get_program(), "bwrap");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args[args.len() - 2], "/bin/echo");
        assert_eq!(args[args.len() - 1], payload[0].as_str());
    }
}
