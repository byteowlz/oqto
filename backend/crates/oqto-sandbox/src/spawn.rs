use anyhow::Result;
use std::path::Path;

use crate::SandboxConfig;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;

/// Configure bwrap child pre-exec hardening hooks.
///
/// Applies:
/// - PR_SET_NO_NEW_PRIVS when enabled
/// - Seccomp fd wiring to descriptor 3 for bwrap `--seccomp 3`
///
/// Note: Landlock is NOT applied here. It is installed by the inner shim
/// (`crate::shim`) after bwrap completes user-namespace setup, because
/// Landlock write restrictions applied before exec block bwrap's write to
/// `/proc/self/uid_map`. See trx oqto-b4za for context.
#[cfg(target_os = "linux")]
pub fn configure_bwrap_pre_exec(
    cmd: &mut std::process::Command,
    config: &SandboxConfig,
    _workspace: &Path,
) -> Result<()> {
    let seccomp_file = config.open_seccomp_bpf_file(None)?;
    let seccomp_fd = seccomp_file.as_ref().map(AsRawFd::as_raw_fd);
    let no_new_privs = config.no_new_privs;

    if seccomp_fd.is_some() || no_new_privs {
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
