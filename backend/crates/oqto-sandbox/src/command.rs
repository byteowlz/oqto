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
    build_sandbox_command_with_cwd(config, workspace, Path::new("."), program, args, stdin)
}

/// Select cwd without changing workspace or mount authority. Relative paths are
/// resolved under workspace; outside-workspace cwd is rejected.
pub fn build_sandbox_command_with_cwd(
    config: &SandboxConfig,
    workspace: &Path,
    cwd: &Path,
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
    let cwd = resolve_cwd(&workspace, cwd)?;
    platform_command(config, &workspace, &cwd, program, args, stdin)
}

pub(crate) fn resolve_cwd(workspace: &Path, cwd: &Path) -> Result<std::path::PathBuf> {
    let cwd = if cwd.is_absolute() {
        cwd.to_path_buf()
    } else {
        workspace.join(cwd)
    };
    let cwd = cwd
        .canonicalize()
        .context("resolving sandbox execution directory")?;
    ensure!(cwd.is_dir(), "sandbox cwd is not a directory");
    ensure!(
        cwd.starts_with(workspace),
        "sandbox cwd must remain inside the authorized workspace"
    );
    Ok(cwd)
}

#[cfg(target_os = "linux")]
fn platform_command(
    config: &SandboxConfig,
    workspace: &Path,
    cwd: &Path,
    program: &Path,
    args: &[String],
    _stdin: SandboxStdin,
) -> Result<Command> {
    ensure!(
        SandboxConfig::is_bwrap_available(),
        "bubblewrap (bwrap) is not installed or not runnable; refusing unsandboxed execution"
    );
    let mut sandbox_args = config
        .build_bwrap_args_for_user(workspace, None)
        .context("sandbox policy rejected; refusing unsandboxed execution")?;
    let cwd_value = cwd.to_str().context("sandbox cwd must be UTF-8")?;
    let chdir = sandbox_args
        .iter()
        .position(|arg| arg == "--chdir")
        .context("compiled sandbox command lacks cwd")?;
    let slot = sandbox_args
        .get_mut(chdir + 1)
        .context("compiled sandbox cwd lacks value")?;
    *slot = cwd_value.to_owned();
    let mut command = Command::new("bwrap");
    command.args(sandbox_args).arg(program).args(args);
    Ok(command)
}

#[cfg(all(target_os = "macos", feature = "macos-seatbelt"))]
fn platform_command(
    config: &SandboxConfig,
    workspace: &Path,
    cwd: &Path,
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
        .current_dir(cwd);
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
    fn cwd_resolves_inside_workspace_without_widening() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let workspace = temp.path().canonicalize()?;
        std::fs::create_dir(workspace.join("nested"))?;
        assert_eq!(
            resolve_cwd(&workspace, Path::new("nested"))?,
            workspace.join("nested")
        );
        assert!(resolve_cwd(&workspace, Path::new("..")).is_err());
        assert!(resolve_cwd(&workspace, Path::new("missing")).is_err());
        std::fs::write(workspace.join("file"), b"not a directory")?;
        assert!(resolve_cwd(&workspace, Path::new("file")).is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn cwd_symlink_cannot_escape_workspace() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let workspace = temp.path().canonicalize()?;
        std::os::unix::fs::symlink(outside.path(), workspace.join("escape"))?;
        assert!(resolve_cwd(&workspace, Path::new("escape")).is_err());
        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cwd_changes_only_chdir_not_mount_grants() -> Result<()> {
        if !SandboxConfig::is_bwrap_available() {
            return Ok(());
        }
        let temp = tempfile::tempdir()?;
        std::fs::create_dir(temp.path().join("nested"))?;
        let config = SandboxConfig::from_profile("minimal");
        let base = build_sandbox_command(
            &config,
            temp.path(),
            Path::new("/bin/true"),
            &[],
            SandboxStdin::Redirected,
        )?;
        let changed = build_sandbox_command_with_cwd(
            &config,
            temp.path(),
            Path::new("nested"),
            Path::new("/bin/true"),
            &[],
            SandboxStdin::Redirected,
        )?;
        let base: Vec<_> = base.get_args().map(|arg| arg.to_os_string()).collect();
        let mut changed: Vec<_> = changed.get_args().map(|arg| arg.to_os_string()).collect();
        let index = base
            .iter()
            .position(|arg| arg == "--chdir")
            .context("missing chdir")?;
        assert_eq!(
            changed[index + 1],
            temp.path().canonicalize()?.join("nested")
        );
        changed[index + 1] = base[index + 1].clone();
        assert_eq!(base, changed);
        Ok(())
    }

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
