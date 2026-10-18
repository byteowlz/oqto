//! Private local ExecutionEnv transport launcher. No unrestricted fallback.
//! The trusted owner supplies a private spec; model tool arguments cannot alter it.
use anyhow::{Context, Result, ensure};
use oqto_sandbox::{
    NetworkConfig, NetworkMode, SandboxConfig, SandboxStdin, SeccompMode, build_sandbox_command,
    configure_bwrap_pre_exec,
};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HandsSpec {
    workspace: PathBuf,
    daemon: PathBuf,
    bpf: PathBuf,
    protected_roots: Vec<PathBuf>,
    /// Operator-approved extra resources, compiled by the guard from active
    /// grants. Never derived from model arguments without approval.
    #[serde(default)]
    read_paths: Vec<PathBuf>,
    #[serde(default)]
    write_paths: Vec<PathBuf>,
    #[serde(default)]
    network: bool,
    /// Ceiling deny roots: mounted empty so tools can never touch them, even
    /// inside the workspace, an approved grant or a pre-approved tree.
    #[serde(default)]
    mask_paths: Vec<PathBuf>,
}

/// A grant may neither sit inside a protected root nor contain one.
fn overlaps(path: &std::path::Path, protected: &std::path::Path) -> bool {
    path.starts_with(protected) || protected.starts_with(path)
}

#[cfg(target_os = "linux")]
fn main() -> Result<()> {
    use std::os::unix::{fs::MetadataExt, process::CommandExt};
    tracing_subscriber::fmt()
        .with_max_level(tracing_subscriber::filter::LevelFilter::WARN)
        .with_writer(std::io::stderr)
        .init();
    let mut args = std::env::args_os().skip(1);
    let spec_path = PathBuf::from(args.next().context("private hands spec required")?);
    let metadata = std::fs::symlink_metadata(&spec_path)?;
    ensure!(
        metadata.is_file()
            && metadata.mode() & 0o077 == 0
            && metadata.uid() == std::fs::metadata("/proc/self")?.uid(),
        "hands spec must be an owned private regular file"
    );
    let parent = spec_path.parent().context("spec parent required")?;
    let directory = std::fs::symlink_metadata(parent)?;
    ensure!(
        directory.is_dir() && directory.mode() & 0o077 == 0 && directory.uid() == metadata.uid(),
        "hands spec parent must be private and owned consistently"
    );
    // The parent's actor boundary is supplied by the runner. This transport is
    // not a replacement for multi-user Account authentication.
    ensure!(metadata.len() <= 16384, "hands spec too large");
    let spec: HandsSpec = serde_json::from_slice(&std::fs::read(&spec_path)?)?;
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
    config.allow_read.clear();
    config.allow_write.clear();
    config.extra_ro_bind = vec![daemon.display().to_string()];
    config.extra_rw_bind.clear();
    config.workspace_cache_enabled = false;
    // oqt executors are not Pi/Claude harness sessions: never create or bind
    // history shards inside the owner's ~/.pi or ~/.claude (preserve-first).
    config.scoped_paths.clear();
    config.isolate_network = true;
    config.seccomp_mode = SeccompMode::Enforce;
    config.seccomp_bpf_path = Some(spec.bpf.canonicalize()?.display().to_string());
    let mut protected_roots = Vec::new();
    for path in spec.protected_roots {
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
        // Deny entries are mounted empty (tmpfs), so a masked subpath
        // disappears from the workspace, grants and pre-approved trees alike.
        config.deny_read.push(masked.display().to_string());
        config.deny_write.push(masked.display().to_string());
    }
    if spec.network {
        config.isolate_network = false;
        config.network = Some(NetworkConfig {
            mode: NetworkMode::Open,
            ..NetworkConfig::default()
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

#[cfg(not(target_os = "linux"))]
fn main() -> Result<()> {
    anyhow::bail!("oqt hands currently require Linux; no fallback")
}
