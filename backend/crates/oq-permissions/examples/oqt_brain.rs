//! Confined launcher for the oqt session worker ("brain"). No unrestricted
//! fallback. The trusted guard writes a private spec; the brain can only reach
//! explicitly bound paths/sockets. Owner credentials, the permission ledger and
//! guard state are never bindable: approval authority stays outside the brain.
use anyhow::{Context, Result, ensure};
use oqto_sandbox::{
    NetworkConfig, NetworkMode, SandboxConfig, SandboxStdin, SeccompMode, build_sandbox_command,
    configure_bwrap_pre_exec,
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrainSpec {
    node: PathBuf,
    cwd: PathBuf,
    ro: Vec<PathBuf>,
    rw: Vec<PathBuf>,
    forbidden: Vec<PathBuf>,
    bpf: PathBuf,
    env: BTreeMap<String, String>,
    args: Vec<String>,
}

fn env_allowed(key: &str) -> bool {
    (key.starts_with("PI_") || key.starts_with("OQT_") || key.starts_with("__PI_"))
        && key.len() <= 128
        && key
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        || key == "NODE_OPTIONS"
}

/// A bind may not expose a forbidden path, either by being inside it or by
/// being an ancestor that contains it.
fn overlaps(bind: &Path, forbidden: &Path) -> bool {
    bind.starts_with(forbidden) || forbidden.starts_with(bind)
}

#[cfg(target_os = "linux")]
fn main() -> Result<()> {
    use std::os::unix::{fs::MetadataExt, process::CommandExt};
    tracing_subscriber::fmt()
        .with_max_level(tracing_subscriber::filter::LevelFilter::WARN)
        .with_writer(std::io::stderr)
        .init();
    let mut args = std::env::args_os().skip(1);
    let spec_path = PathBuf::from(args.next().context("private brain spec required")?);
    ensure!(args.next().is_none(), "unexpected brain launcher arguments");
    let uid = std::fs::metadata("/proc/self")?.uid();
    let metadata = std::fs::symlink_metadata(&spec_path)?;
    ensure!(
        metadata.is_file() && metadata.mode() & 0o077 == 0 && metadata.uid() == uid,
        "brain spec must be an owned private regular file"
    );
    let parent = spec_path.parent().context("spec parent required")?;
    let directory = std::fs::symlink_metadata(parent)?;
    ensure!(
        directory.is_dir() && directory.mode() & 0o077 == 0 && directory.uid() == uid,
        "brain spec parent must be private and owned"
    );
    ensure!(metadata.len() <= 65536, "brain spec too large");
    let spec: BrainSpec = serde_json::from_slice(&std::fs::read(&spec_path)?)?;
    ensure!(
        spec.ro.len() + spec.rw.len() <= 64 && spec.forbidden.len() <= 64,
        "bounded binds required"
    );
    ensure!(!spec.forbidden.is_empty(), "forbidden roots required");
    ensure!(
        spec.args.len() <= 16 && spec.env.len() <= 64,
        "bounded payload required"
    );
    let node = spec.node.canonicalize()?;
    ensure!(node.is_file(), "node must be a file");
    let cwd = spec.cwd.canonicalize()?;
    ensure!(cwd.is_dir(), "brain cwd must be a directory");
    let forbidden: Vec<PathBuf> = spec
        .forbidden
        .iter()
        .map(|path| {
            // A missing forbidden path still fences its lexical location.
            path.canonicalize().unwrap_or_else(|_| path.clone())
        })
        .collect();
    let resolve = |paths: &[PathBuf]| -> Result<Vec<String>> {
        paths
            .iter()
            .map(|path| {
                let resolved = path
                    .canonicalize()
                    .with_context(|| format!("resolving brain bind {}", path.display()))?;
                ensure!(
                    resolved != Path::new("/"),
                    "refusing to bind the filesystem root"
                );
                for denied in &forbidden {
                    ensure!(
                        !overlaps(&resolved, denied),
                        "brain bind {} overlaps protected {}",
                        resolved.display(),
                        denied.display()
                    );
                }
                Ok(resolved.display().to_string())
            })
            .collect()
    };
    let ro = resolve(&spec.ro)?;
    let rw = resolve(&spec.rw)?;
    for denied in &forbidden {
        ensure!(!overlaps(&cwd, denied), "brain cwd overlaps protected root");
    }
    for key in spec.env.keys() {
        ensure!(env_allowed(key), "brain environment key not allowed: {key}");
    }
    let mut config = SandboxConfig::strict();
    config.allow_read.clear();
    config.allow_write.clear();
    // Grants go through the policy allow lists so bwrap mounts and the
    // Landlock ruleset agree; bind lists alone are not writable under Landlock.
    config.allow_read = ro;
    config.allow_read.push(node.display().to_string());
    config.allow_write = rw;
    config.extra_ro_bind.clear();
    config.extra_rw_bind.clear();
    config.workspace_cache_enabled = false;
    // oqt executors are not Pi/Claude harness sessions: never create or bind
    // history shards inside the owner's ~/.pi or ~/.claude (preserve-first).
    config.scoped_paths.clear();
    // The brain calls model providers; it holds no owner secrets or authority.
    config.isolate_network = false;
    config.network = Some(NetworkConfig {
        mode: NetworkMode::Open,
        ..NetworkConfig::default()
    });
    config.seccomp_mode = SeccompMode::Enforce;
    config.seccomp_bpf_path = Some(spec.bpf.canonicalize()?.display().to_string());
    for denied in &forbidden {
        config.deny_read.push(denied.display().to_string());
        config.deny_write.push(denied.display().to_string());
    }
    let mut command =
        build_sandbox_command(&config, &cwd, &node, &spec.args, SandboxStdin::Redirected)?;
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/tmp")
        .env("LANG", "C.UTF-8")
        .envs(&spec.env);
    configure_bwrap_pre_exec(&mut command, &config, &cwd, None)?;
    Err(command.exec()).context("executing confined brain")
}

#[cfg(not(target_os = "linux"))]
fn main() -> Result<()> {
    anyhow::bail!("oqt brain confinement currently requires Linux; no fallback")
}
