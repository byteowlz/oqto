use anyhow::Result;
use std::path::Path;

use crate::SandboxConfig;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[cfg(target_os = "linux")]
use std::ffi::CString;

/// Configure bwrap child pre-exec hardening hooks.
///
/// Applies:
/// - PR_SET_NO_NEW_PRIVS when enabled
/// - Seccomp fd wiring to descriptor 198 for bwrap `--seccomp 198`
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
    let seccomp_path_cstr = if seccomp_file.is_some() {
        let path = config
            .resolve_seccomp_bpf_path(None)
            .ok_or_else(|| anyhow::anyhow!("seccomp policy path missing after validation"))?;
        let raw = path.to_string_lossy().into_owned();
        Some(CString::new(raw).map_err(|_| anyhow::anyhow!("seccomp path contains NUL byte"))?)
    } else {
        None
    };
    let no_new_privs = config.no_new_privs;

    if seccomp_path_cstr.is_some() || no_new_privs {
        // SAFETY: pre_exec runs in child after fork, before exec.
        unsafe {
            cmd.pre_exec(move || {
                if no_new_privs {
                    let rc = libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
                    if rc != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }

                if let Some(path) = seccomp_path_cstr.as_ref() {
                    let fd = libc::open(path.as_ptr(), libc::O_RDONLY);
                    if fd == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if libc::dup2(fd, 198) == -1 {
                        let err = std::io::Error::last_os_error();
                        libc::close(fd);
                        return Err(err);
                    }
                    libc::close(fd);
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
