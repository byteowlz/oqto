//! Private local ExecutionEnv transport launcher. No unrestricted fallback.
//! The trusted owner supplies a private spec; model tool arguments cannot alter it.
use anyhow::{Context, Result, ensure};
use oqto_sandbox::{
    SandboxConfig, SandboxStdin, SeccompMode, build_sandbox_command, configure_bwrap_pre_exec,
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
}

#[cfg(target_os = "linux")]
fn main() -> Result<()> {
    use std::os::unix::{fs::MetadataExt, process::CommandExt};
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
    config.isolate_network = true;
    config.seccomp_mode = SeccompMode::Enforce;
    config.seccomp_bpf_path = Some(spec.bpf.canonicalize()?.display().to_string());
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
