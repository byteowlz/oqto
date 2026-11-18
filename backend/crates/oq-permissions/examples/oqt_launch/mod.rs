//! Shared spec handling for the oqt brain and hands launchers.
//!
//! Both launchers read a private spec written by the trusted guard and build
//! one confined process from it. The platform part differs: Linux execs into
//! bwrap (PID namespace, seccomp, Landlock); macOS compiles a Seatbelt profile
//! through oqto-sandbox and stays the confined child's supervising parent,
//! because macOS has no PID namespace to end the tree.
use anyhow::{Context, Result, ensure};
use oqto_sandbox::process_tree::current_uid;
use std::path::Path;

/// Read a spec that only the owner could have written: an owned 0600 regular
/// file in an owned 0700 directory, at most `limit` bytes.
pub fn read_private_spec(path: &Path, limit: u64) -> Result<Vec<u8>> {
    use std::os::unix::fs::MetadataExt;
    let uid = current_uid();
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && metadata.mode() & 0o077 == 0 && metadata.uid() == uid,
        "spec must be an owned private regular file"
    );
    let parent = path.parent().context("spec parent required")?;
    let directory = std::fs::symlink_metadata(parent)?;
    ensure!(
        directory.is_dir() && directory.mode() & 0o077 == 0 && directory.uid() == uid,
        "spec parent must be private and owned"
    );
    ensure!(metadata.len() <= limit, "spec too large");
    Ok(std::fs::read(path)?)
}

/// Seatbelt is a path filter, not a namespace. It cannot provide a PID
/// namespace, capability dropping, user-namespace denial, no_new_privs,
/// seccomp or Landlock, and oqto-sandbox refuses a profile that asks for
/// them. These are switched off explicitly here, not silently downgraded:
/// on macOS the guarantee is the Seatbelt path and network policy plus the
/// launcher's process-tree supervision. There is no seccomp on macOS.
#[cfg(target_os = "macos")]
pub fn seatbelt_compatible(config: &mut oqto_sandbox::SandboxConfig) {
    use oqto_sandbox::{LandlockMode, SeccompMode};
    config.isolate_pid = false;
    config.drop_all_caps = false;
    config.disable_userns = false;
    config.assert_userns_disabled = false;
    config.no_new_privs = false;
    config.seccomp_mode = SeccompMode::Off;
    config.seccomp_bpf_path = None;
    config.landlock_mode = LandlockMode::Off;
    config.overlay_enabled = false;
    // macOS resolves host names through this one Unix socket, not with a direct
    // DNS query. A launch with open network gets it, as the shipped macOS host
    // template grants it; without it every connection to a host name fails
    // with ENOTFOUND. An explicit deny of the path still wins.
    if !config.isolate_network
        && config.network_mode() != oqto_sandbox::NetworkMode::Isolated
        && !config
            .allow_read
            .iter()
            .any(|path| path == SYSTEM_RESOLVER_SOCKET)
    {
        config.allow_read.push(SYSTEM_RESOLVER_SOCKET.to_string());
    }
}

/// The macOS system resolver's socket (mDNSResponder).
#[cfg(target_os = "macos")]
const SYSTEM_RESOLVER_SOCKET: &str = "/private/var/run/mDNSResponder";

/// Build the `sandbox-exec` invocation: the profile oqto-sandbox compiles for
/// `config`, followed by the launch marker's rules. No PATH lookup, no shell.
#[cfg(target_os = "macos")]
pub fn seatbelt_command(
    config: &oqto_sandbox::SandboxConfig,
    workspace: &Path,
    marker: &oqto_sandbox::process_tree::Marker,
    program: &Path,
    args: &[String],
) -> Result<std::process::Command> {
    let mut profile = oqto_sandbox::seatbelt::compile_profile(config, workspace)?;
    profile.push_str(&marker.profile_rules());
    let mut command = std::process::Command::new("/usr/bin/sandbox-exec");
    command
        .arg("-p")
        .arg(profile)
        .arg(program)
        .args(args)
        .current_dir(workspace)
        .env_clear();
    Ok(command)
}

/// A private scratch directory for TMPDIR/HOME. Seatbelt has no private /tmp,
/// so the guard supplies one that is neither the workspace nor protected.
#[cfg(target_os = "macos")]
pub fn scratch(
    path: Option<&std::path::PathBuf>,
    protected: &[std::path::PathBuf],
) -> Result<std::path::PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let path = path
        .context("macOS confinement requires a private scratch directory")?
        .canonicalize()?;
    let metadata = std::fs::symlink_metadata(&path)?;
    ensure!(
        metadata.is_dir() && metadata.mode() & 0o077 == 0 && metadata.uid() == current_uid(),
        "scratch must be an owned private directory"
    );
    for root in protected {
        ensure!(
            !(path.starts_with(root) || root.starts_with(&path)),
            "scratch {} overlaps protected {}",
            path.display(),
            root.display()
        );
    }
    Ok(path)
}
