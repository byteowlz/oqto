use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::{LandlockMode, SandboxConfig};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;

/// Configure bwrap child pre-exec hardening hooks.
///
/// Applies:
/// - PR_SET_NO_NEW_PRIVS when enabled
/// - Landlock write restrictions (when enabled and compatible)
/// - Seccomp fd wiring to descriptor 3 for bwrap `--seccomp 3`
///
/// This helper centralizes pre-exec behavior so all bwrap spawn paths share
/// identical semantics.
#[cfg(target_os = "linux")]
pub fn configure_bwrap_pre_exec(
    cmd: &mut std::process::Command,
    config: &SandboxConfig,
    workspace: &Path,
) -> Result<()> {
    let seccomp_file = config.open_seccomp_bpf_file(None)?;
    let seccomp_fd = seccomp_file.as_ref().map(AsRawFd::as_raw_fd);
    let no_new_privs = config.no_new_privs;

    // Skip Landlock in pre_exec when --unshare-user is active: bwrap needs to
    // write /proc/self/uid_map during namespace setup, and Landlock write
    // restrictions applied before exec block that write.
    let apply_landlock_pre_exec = !config.disable_userns;
    let landlock_cfg = config.clone();
    let landlock_workspace: PathBuf = workspace.to_path_buf();

    if seccomp_fd.is_some()
        || no_new_privs
        || (apply_landlock_pre_exec && landlock_cfg.landlock_mode != LandlockMode::Off)
    {
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

                if apply_landlock_pre_exec {
                    landlock_cfg.apply_landlock(&landlock_workspace, None)?;
                }

                if let Some(fd) = seccomp_fd
                    && libc::dup2(fd, 3) == -1
                {
                    return Err(std::io::Error::last_os_error());
                }

                Ok(())
            });
        }
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn configure_bwrap_pre_exec(
    _cmd: &mut std::process::Command,
    _config: &SandboxConfig,
    _workspace: &Path,
) -> Result<()> {
    Ok(())
}
