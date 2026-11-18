//! Private local ExecutionEnv transport launcher. No unrestricted fallback.
//! The trusted owner supplies a private spec; model tool arguments cannot alter it.
//!
//! Linux: bwrap with seccomp and Landlock; the PID namespace ends every
//! descendant. macOS: a Seatbelt profile compiled by oqto-sandbox, with this
//! launcher staying the supervising parent and reaping the launch's marked
//! processes (see `oqto_sandbox::process_tree`). There is no seccomp on macOS.
//!
//! `oqt_hands reap DIR` (macOS) stops and confirms every process carrying the
//! marker in DIR, for the guard's own confirmation after a stop.
mod oqt_launch;

use anyhow::{Context, Result, ensure};
use oqto_sandbox::SandboxConfig;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HandsSpec {
    workspace: PathBuf,
    daemon: PathBuf,
    /// Compiled seccomp filter: required on Linux, absent on macOS.
    #[serde(default)]
    bpf: Option<PathBuf>,
    protected_roots: Vec<PathBuf>,
    /// Operator-approved extra resources, compiled by the guard from active
    /// grants. Never derived from model arguments without approval.
    #[serde(default)]
    read_paths: Vec<PathBuf>,
    #[serde(default)]
    write_paths: Vec<PathBuf>,
    #[serde(default)]
    network: bool,
    /// Ceiling deny roots: mounted empty (Linux) or unreadable (macOS) so
    /// tools can never touch them, even inside the workspace, an approved
    /// grant or a pre-approved tree.
    #[serde(default)]
    mask_paths: Vec<PathBuf>,
    /// macOS: directory holding the launch marker files.
    #[serde(default)]
    marker: Option<PathBuf>,
    /// macOS: private TMPDIR/HOME (Seatbelt has no private /tmp).
    #[serde(default)]
    scratch: Option<PathBuf>,
}

/// A grant may neither sit inside a protected root nor contain one.
fn overlaps(path: &std::path::Path, protected: &std::path::Path) -> bool {
    path.starts_with(protected) || protected.starts_with(path)
}

struct Launch {
    config: SandboxConfig,
    workspace: PathBuf,
    daemon: PathBuf,
    payload: Vec<String>,
    protected_roots: Vec<PathBuf>,
    spec: HandsSpec,
}

fn prepare(spec_path: PathBuf, args: impl Iterator<Item = std::ffi::OsString>) -> Result<Launch> {
    // The parent's actor boundary is supplied by the runner. This transport is
    // not a replacement for multi-user Account authentication.
    let spec: HandsSpec =
        serde_json::from_slice(&oqt_launch::read_private_spec(&spec_path, 16384)?)?;
    ensure!(
        !spec.protected_roots.is_empty() && spec.protected_roots.len() <= 32,
        "protected roots required and bounded"
    );
    let workspace = spec.workspace.canonicalize()?;
    ensure!(
        workspace.is_dir() && workspace != std::path::Path::new("/"),
        "invalid work directory"
    );
    let daemon = spec.daemon.canonicalize()?;
    ensure!(daemon.is_file(), "daemon must be a file");
    let mut config = SandboxConfig::strict();
    // Hosts without an Oqto installation lack /etc/oqto and friends.
    config.prune_absent_system_denies();
    config.allow_read.clear();
    config.allow_write.clear();
    config.extra_ro_bind = vec![daemon.display().to_string()];
    config.extra_rw_bind.clear();
    config.workspace_cache_enabled = false;
    // oqt executors are not Pi/Claude harness sessions: never create or bind
    // history shards inside the owner's ~/.pi or ~/.claude (preserve-first).
    config.scoped_paths.clear();
    config.isolate_network = true;
    let mut protected_roots = Vec::new();
    for path in &spec.protected_roots {
        let protected = path
            .canonicalize()
            .with_context(|| format!("resolving protected root {}", path.display()))?;
        ensure!(
            !workspace.starts_with(&protected),
            "work directory inside protected authority/config root"
        );
        config.deny_read.push(protected.display().to_string());
        config.deny_write.push(protected.display().to_string());
        protected_roots.push(protected);
    }
    ensure!(
        spec.read_paths.len() + spec.write_paths.len() <= 32,
        "too many granted resources"
    );
    let granted = |paths: &[PathBuf], target: &mut Vec<String>| -> Result<()> {
        for path in paths {
            let resolved = path
                .canonicalize()
                .with_context(|| format!("resolving granted path {}", path.display()))?;
            ensure!(
                resolved != std::path::Path::new("/") && !workspace.starts_with(&resolved),
                "granted path must not contain the work directory or root"
            );
            for protected in &protected_roots {
                ensure!(
                    !overlaps(&resolved, protected),
                    "granted path {} overlaps protected root {}",
                    resolved.display(),
                    protected.display()
                );
            }
            target.push(resolved.display().to_string());
        }
        Ok(())
    };
    // Through the policy allow lists so bwrap and Landlock agree on access.
    granted(&spec.read_paths, &mut config.allow_read)?;
    granted(&spec.write_paths, &mut config.allow_write)?;
    ensure!(spec.mask_paths.len() <= 64, "too many masked paths");
    for path in &spec.mask_paths {
        let resolved = oq_permissions::normalize_lexical(path)?;
        for protected in &protected_roots {
            ensure!(
                !overlaps(&resolved, protected),
                "masked path {} overlaps protected root {}",
                resolved.display(),
                protected.display()
            );
        }
        // A mask only means something where the tools could actually see
        // content: inside the workspace or a granted tree. Elsewhere the path
        // is invisible anyway. bwrap must be able to create the empty
        // mountpoint, so a mask needs to exist already or sit in the
        // (writable) workspace; a non-existent path under a read-only grant
        // has nothing to hide and cannot be pre-empted.
        let inside = |root: &std::path::Path| resolved.starts_with(root);
        let in_workspace = inside(&workspace);
        let in_grant = spec
            .read_paths
            .iter()
            .chain(&spec.write_paths)
            .any(|grant| inside(&resolved) || resolved.starts_with(grant));
        if !in_workspace && !in_grant {
            continue;
        }
        // Non-existent under a read-only grant: nothing to hide, and bwrap
        // could not create the mountpoint anyway.
        if !in_workspace && !resolved.exists() {
            continue;
        }
        let masked = if resolved.exists() {
            resolved
                .canonicalize()
                .with_context(|| format!("resolving masked path {}", resolved.display()))?
        } else {
            resolved
        };
        if !in_workspace && !masked.starts_with(workspace.as_path()) {
            let granted = spec
                .read_paths
                .iter()
                .chain(&spec.write_paths)
                .any(|grant| {
                    let grant = grant.canonicalize().unwrap_or_else(|_| grant.to_path_buf());
                    masked.starts_with(&grant)
                });
            if !granted {
                continue;
            }
        }
        // Deny entries are mounted empty (tmpfs) on Linux and unreadable on
        // macOS, so a masked subpath is unusable in the workspace, grants and
        // pre-approved trees alike.
        config.deny_read.push(masked.display().to_string());
        config.deny_write.push(masked.display().to_string());
    }
    if spec.network {
        config.isolate_network = false;
        config.network = Some(oqto_sandbox::NetworkConfig {
            mode: oqto_sandbox::NetworkMode::Open,
            ..oqto_sandbox::NetworkConfig::default()
        });
    }
    let payload: Vec<String> = args
        .map(|arg| {
            arg.into_string()
                .map_err(|_| anyhow::anyhow!("non-UTF8 daemon argument"))
        })
        .collect::<Result<_>>()?;
    ensure!(
        payload.len() == 3
            && payload[0] == "serve"
            && payload[1] == "--token"
            && payload[2].len() == 32
            && payload[2].bytes().all(|c| c.is_ascii_hexdigit()),
        "only framed daemon startup allowed"
    );
    Ok(Launch {
        config,
        workspace,
        daemon,
        payload,
        protected_roots,
        spec,
    })
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing_subscriber::filter::LevelFilter::WARN)
        .with_writer(std::io::stderr)
        .init();
    let mut args = std::env::args_os().skip(1);
    let first = args.next().context("private hands spec required")?;
    if first == "reap" {
        let directory = PathBuf::from(args.next().context("marker directory required")?);
        ensure!(args.next().is_none(), "unexpected reap arguments");
        return reap(&directory);
    }
    launch(prepare(PathBuf::from(first), args)?)
}

#[cfg(target_os = "linux")]
fn launch(launch: Launch) -> Result<()> {
    use oqto_sandbox::{
        SandboxStdin, SeccompMode, build_sandbox_command, configure_bwrap_pre_exec,
    };
    use std::os::unix::process::CommandExt;
    let Launch {
        mut config,
        workspace,
        daemon,
        payload,
        spec,
        ..
    } = launch;
    ensure!(
        spec.marker.is_none() && spec.scratch.is_none(),
        "macOS-only hands fields on Linux"
    );
    let bpf = spec
        .bpf
        .context("Linux hands require a compiled seccomp filter")?;
    config.seccomp_mode = SeccompMode::Enforce;
    config.seccomp_bpf_path = Some(bpf.canonicalize()?.display().to_string());
    let mut command = build_sandbox_command(
        &config,
        &workspace,
        &daemon,
        &payload,
        SandboxStdin::Redirected,
    )?;
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/tmp")
        .env("LANG", "C.UTF-8")
        .env("SHELL", "/bin/bash");
    configure_bwrap_pre_exec(&mut command, &config, &workspace, None)?;
    Err(command.exec()).context("executing confined hands")
}

#[cfg(target_os = "linux")]
fn reap(_: &std::path::Path) -> Result<()> {
    anyhow::bail!("reap is macOS-only; Linux hands end with their PID namespace")
}

#[cfg(target_os = "macos")]
fn launch(launch: Launch) -> Result<()> {
    use oqto_sandbox::process_tree::{Marker, supervise};
    let Launch {
        mut config,
        workspace,
        daemon,
        payload,
        mut protected_roots,
        spec,
    } = launch;
    ensure!(
        spec.bpf.is_none(),
        "there is no seccomp on macOS; refusing a filter it would not apply"
    );
    let marker = Marker::open(
        spec.marker
            .as_deref()
            .context("macOS hands require a launch marker")?,
    )?;
    protected_roots.push(workspace.clone());
    let scratch = oqt_launch::scratch(spec.scratch.as_ref(), &protected_roots)?;
    oqt_launch::seatbelt_compatible(&mut config);
    config.allow_write.push(scratch.display().to_string());
    let mut command =
        oqt_launch::seatbelt_command(&config, &workspace, &marker, &daemon, &payload)?;
    command
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &scratch)
        .env("TMPDIR", &scratch)
        .env("LANG", "C.UTF-8")
        .env("SHELL", "/bin/bash");
    let code = supervise(command, &marker, std::time::Duration::from_millis(500))?;
    std::process::exit(code)
}

#[cfg(target_os = "macos")]
fn reap(directory: &std::path::Path) -> Result<()> {
    use oqto_sandbox::process_tree::{Marker, reap};
    let report = reap(&Marker::open(directory)?, std::time::Duration::from_secs(3))?;
    println!(
        "{}",
        serde_json::json!({
            "confirmed": report.confirmed(),
            "killed": report.killed,
            "remaining": report.remaining,
            "unverifiable": report.unverifiable,
        })
    );
    ensure!(report.confirmed(), "launch not confirmed stopped");
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn launch(_: Launch) -> Result<()> {
    anyhow::bail!("oqt hands require Linux or macOS; no fallback")
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn reap(_: &std::path::Path) -> Result<()> {
    anyhow::bail!("oqt hands require Linux or macOS")
}
