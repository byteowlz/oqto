//! Inner-process entry point that applies Landlock after bwrap finishes
//! user-namespace setup, then execs the real target command.
//!
//! Why this exists: applying Landlock in the outer `pre_exec` hook (before
//! bwrap runs) blocks bwrap from writing `/proc/self/uid_map` during
//! `--unshare-user`, which aborts the sandbox. The previous fix was to skip
//! Landlock whenever `disable_userns=true`, which silently disabled Landlock
//! in every built-in profile (trx: oqto-b4za).
//!
//! Now: the outer `oqto-sandbox` binary wires the bwrap command so that
//! bwrap's inner command is a re-exec of `oqto-sandbox` itself in shim mode.
//! The shim runs after namespace setup is complete, installs Landlock rules,
//! then `execvp`s the real command.

use anyhow::{Context, Result};
use std::env;
use std::path::PathBuf;

use crate::config::{LandlockMode, SandboxConfig};

pub const SHIM_ENV: &str = "OQTO_SANDBOX_SHIM_MODE";
pub const ENV_MODE: &str = "OQTO_LANDLOCK_MODE";
pub const ENV_WORKSPACE: &str = "OQTO_LANDLOCK_WORKSPACE";
pub const ENV_ALLOW_WRITE: &str = "OQTO_LANDLOCK_ALLOW_WRITE";

/// Mount path where the outer process binds the oqto-sandbox binary so the
/// shim can re-exec itself inside bwrap.
pub const SHIM_MOUNT_PATH: &str = "/.oqto-sandbox-shim";

/// Override for the shim binary source (absolute path on the host).
/// Takes precedence over PATH lookup and `current_exe()`.
pub const SHIM_BIN_OVERRIDE_ENV: &str = "OQTO_SANDBOX_SHIM_BIN";

/// If the shim sentinel env var is set, apply Landlock and exec the remainder
/// of argv. Otherwise return `Ok(())` so normal CLI flow continues.
pub fn maybe_run_shim() -> Result<()> {
    if env::var(SHIM_ENV).ok().as_deref() != Some("1") {
        return Ok(());
    }

    #[cfg(not(target_os = "linux"))]
    {
        anyhow::bail!("{} set on non-Linux platform", SHIM_ENV);
    }

    #[cfg(target_os = "linux")]
    {
        run_shim_linux()
    }
}

/// Resolve the absolute path to a binary that can serve as the Landlock shim.
///
/// Resolution order:
/// 1. `OQTO_SANDBOX_SHIM_BIN` (explicit override)
/// 2. `oqto-sandbox` on `PATH`
/// 3. `current_exe()` when it is named `oqto-sandbox`
pub fn resolve_shim_binary() -> Option<PathBuf> {
    if let Ok(raw) = env::var(SHIM_BIN_OVERRIDE_ENV) {
        let p = PathBuf::from(raw);
        if p.exists() {
            return Some(p);
        }
    }

    if let Ok(path) = env::var("PATH") {
        for dir in path.split(':') {
            if dir.is_empty() {
                continue;
            }
            let candidate = std::path::Path::new(dir).join("oqto-sandbox");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    if let Ok(cur) = env::current_exe()
        && cur
            .file_name()
            .and_then(|f| f.to_str())
            .is_some_and(|s| s == "oqto-sandbox")
    {
        return Some(cur);
    }

    None
}

#[cfg(target_os = "linux")]
fn run_shim_linux() -> Result<()> {
    use std::ffi::CString;

    let mode = match env::var(ENV_MODE).ok().as_deref() {
        Some("audit") => LandlockMode::Audit,
        Some("enforce") => LandlockMode::Enforce,
        _ => LandlockMode::Off,
    };

    if mode != LandlockMode::Off {
        // Landlock's restrict_self requires NoNewPrivs=1 (or CAP_SYS_ADMIN,
        // which we don't have inside the init user namespace). The outer
        // pre_exec hook sets this when the profile enables no_new_privs, but
        // we set it here too so Landlock works even with no_new_privs=false.
        // PR_SET_NO_NEW_PRIVS is sticky across exec, so this is idempotent.
        // SAFETY: prctl with fixed flag constants.
        let rc = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1u64, 0u64, 0u64, 0u64) };
        if rc != 0 {
            return Err(std::io::Error::last_os_error()).context("PR_SET_NO_NEW_PRIVS in shim");
        }

        let workspace: PathBuf = env::var(ENV_WORKSPACE)
            .context("shim mode requires OQTO_LANDLOCK_WORKSPACE")?
            .into();

        let allow_write: Vec<String> = env::var(ENV_ALLOW_WRITE)
            .unwrap_or_default()
            .split(':')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        // Build a minimal config to reuse apply_landlock() instead of
        // duplicating the Landlock syscall logic.
        let mut cfg = SandboxConfig::from_profile("minimal");
        cfg.landlock_mode = mode.clone();
        cfg.allow_write = allow_write;

        cfg.apply_landlock(&workspace, None)
            .context("applying Landlock in inner shim")?;
    }

    // Strip shim env vars so the target command sees a clean environment.
    // SAFETY: process is single-threaded at shim entry.
    unsafe {
        env::remove_var(SHIM_ENV);
        env::remove_var(ENV_MODE);
        env::remove_var(ENV_WORKSPACE);
        env::remove_var(ENV_ALLOW_WRITE);
    }

    let argv: Vec<String> = env::args().skip(1).collect();
    if argv.is_empty() {
        anyhow::bail!("shim mode invoked with no command to exec");
    }

    let cmd = CString::new(argv[0].as_str()).context("command contains NUL byte")?;
    let c_argv: Vec<CString> = argv
        .iter()
        .map(|s| CString::new(s.as_str()).context("argv element contains NUL byte"))
        .collect::<Result<Vec<_>>>()?;
    let mut c_argv_ptrs: Vec<*const libc::c_char> = c_argv.iter().map(|s| s.as_ptr()).collect();
    c_argv_ptrs.push(std::ptr::null());

    // SAFETY: execvp with valid CString-backed argv; on success this function
    // does not return. On failure we fall through and surface errno.
    unsafe {
        libc::execvp(cmd.as_ptr(), c_argv_ptrs.as_ptr());
    }
    Err(std::io::Error::last_os_error()).context("execvp failed in shim")
}
