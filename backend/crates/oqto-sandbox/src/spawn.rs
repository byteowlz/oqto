use anyhow::Result;
use std::path::Path;

use crate::SandboxConfig;
use crate::egress::EgressPlan;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[cfg(target_os = "linux")]
use std::ffi::CString;

/// Configure bwrap child pre-exec hardening hooks.
///
/// Applies, in order, before `exec`:
/// - `setns(CLONE_NEWNET)` into the egress namespace when `egress` is set
///   (proxy mode). Done first, while still privileged, before bwrap unshares
///   its other namespaces; bwrap must not also `--unshare-net` (the arg builder
///   suppresses it for proxy mode) or it would replace this namespace.
/// - PR_SET_NO_NEW_PRIVS when enabled
/// - `setrlimit` resource limits when configured
/// - Seccomp fd wiring to descriptor 198 for bwrap `--seccomp 198`
///
/// Note: Landlock is NOT applied here. It is installed by the inner shim
/// (`crate::landlock_shim`) after bwrap completes user-namespace setup, because
/// Landlock write restrictions applied before exec block bwrap's write to
/// `/proc/self/uid_map`. See trx oqto-b4za for context.
/// Translate configured limits into `(resource, value)` pairs for `setrlimit`.
///
/// Values are clamped to `rlim_t` so a config value larger than the platform's
/// limit type cannot silently wrap into a small limit.
#[cfg(target_os = "linux")]
fn resource_rlimits(config: &SandboxConfig) -> Vec<(libc::__rlimit_resource_t, libc::rlim_t)> {
    let limits = &config.resource_limits;
    [
        (libc::RLIMIT_AS, limits.max_memory_bytes),
        (libc::RLIMIT_NOFILE, limits.max_open_files),
        (libc::RLIMIT_CPU, limits.max_cpu_seconds),
        (libc::RLIMIT_FSIZE, limits.max_file_size_bytes),
    ]
    .into_iter()
    .filter_map(|(resource, value)| {
        value.map(|v| {
            (
                resource,
                libc::rlim_t::try_from(v).unwrap_or(libc::rlim_t::MAX),
            )
        })
    })
    .collect()
}

#[cfg(target_os = "linux")]
pub fn configure_bwrap_pre_exec(
    cmd: &mut std::process::Command,
    config: &SandboxConfig,
    _workspace: &Path,
    egress: Option<&EgressPlan>,
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
    // bwrap execs the payload without resetting rlimits, so limits installed
    // here are inherited by the sandboxed process and its children.
    let rlimits = resource_rlimits(config);
    let netns_path_cstr = match egress {
        Some(plan) => Some(
            CString::new(plan.netns_path())
                .map_err(|_| anyhow::anyhow!("netns path contains NUL byte"))?,
        ),
        None => None,
    };

    if seccomp_path_cstr.is_some()
        || no_new_privs
        || netns_path_cstr.is_some()
        || !rlimits.is_empty()
    {
        // SAFETY: pre_exec runs in child after fork, before exec.
        unsafe {
            cmd.pre_exec(move || {
                // Join the egress namespace first, while still privileged and
                // before bwrap creates its own namespaces.
                if let Some(path) = netns_path_cstr.as_ref() {
                    let fd = libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC);
                    if fd == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if libc::setns(fd, libc::CLONE_NEWNET) == -1 {
                        let err = std::io::Error::last_os_error();
                        libc::close(fd);
                        return Err(err);
                    }
                    libc::close(fd);
                }

                if no_new_privs {
                    let rc = libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
                    if rc != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }

                for (resource, value) in rlimits.iter().copied() {
                    let limit = libc::rlimit {
                        rlim_cur: value,
                        rlim_max: value,
                    };
                    if libc::setrlimit(resource, &limit) != 0 {
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

/// macOS: `setrlimit` is portable, so resource limits apply here too. Seccomp,
/// Landlock and network namespaces have no macOS equivalent and are enforced by
/// the Seatbelt profile or not at all.
#[cfg(target_os = "macos")]
pub fn configure_bwrap_pre_exec(
    cmd: &mut std::process::Command,
    config: &SandboxConfig,
    _workspace: &Path,
    _egress: Option<&EgressPlan>,
) -> Result<()> {
    let limits = &config.resource_limits;
    let rlimits: Vec<(libc::c_int, libc::rlim_t)> = [
        (libc::RLIMIT_AS, limits.max_memory_bytes),
        (libc::RLIMIT_NOFILE, limits.max_open_files),
        (libc::RLIMIT_CPU, limits.max_cpu_seconds),
        (libc::RLIMIT_FSIZE, limits.max_file_size_bytes),
    ]
    .into_iter()
    .filter_map(|(resource, value)| {
        value.map(|v| {
            (
                resource,
                libc::rlim_t::try_from(v).unwrap_or(libc::rlim_t::MAX),
            )
        })
    })
    .collect();

    if rlimits.is_empty() {
        return Ok(());
    }

    // SAFETY: pre_exec runs in the child after fork, before exec.
    unsafe {
        cmd.pre_exec(move || {
            for (resource, value) in rlimits.iter().copied() {
                let limit = libc::rlimit {
                    rlim_cur: value,
                    rlim_max: value,
                };
                if libc::setrlimit(resource, &limit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }

    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn configure_bwrap_pre_exec(
    _cmd: &mut std::process::Command,
    _config: &SandboxConfig,
    _workspace: &Path,
    _egress: Option<&EgressPlan>,
) -> Result<()> {
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::config::ResourceLimits;

    #[test]
    fn unset_limits_produce_no_rlimits() {
        let config = SandboxConfig::default();
        assert!(resource_rlimits(&config).is_empty());
    }

    #[test]
    fn configured_limits_map_to_expected_resources() {
        let config = SandboxConfig {
            resource_limits: ResourceLimits {
                max_memory_bytes: Some(1024),
                max_open_files: Some(64),
                max_cpu_seconds: None,
                max_file_size_bytes: Some(2048),
            },
            ..SandboxConfig::default()
        };

        let limits = resource_rlimits(&config);

        assert_eq!(
            limits,
            vec![
                (libc::RLIMIT_AS, 1024),
                (libc::RLIMIT_NOFILE, 64),
                (libc::RLIMIT_FSIZE, 2048),
            ],
            "only configured limits are installed, CPU stays untouched"
        );
    }

    #[test]
    fn oversized_value_clamps_instead_of_wrapping() {
        let config = SandboxConfig {
            resource_limits: ResourceLimits {
                max_memory_bytes: Some(u64::MAX),
                ..ResourceLimits::default()
            },
            ..SandboxConfig::default()
        };

        let (_, value) = resource_rlimits(&config)[0];
        assert_eq!(value, libc::rlim_t::MAX);
    }
}
