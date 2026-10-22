//! No-launch inspection of the same configuration and compiler used for execution.
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::{SandboxConfig, SandboxStdin, build_sandbox_command_with_cwd};

#[derive(Debug, Serialize)]
pub struct EffectivePolicyReport {
    pub schema: &'static str,
    pub profile: String,
    pub workspace: PathBuf,
    pub cwd: PathBuf,
    /// Conservative persistent write reach from compiled writable bind mounts.
    /// Deny masks and read-only overlays can narrow it; never infer exact access
    /// from this list alone. Source aliases are included to prevent false subset checks.
    pub allow_write: Vec<PathBuf>,
    /// Resolved deny boundaries; descendant exceptions must be evaluated using filesystem_policy.
    pub deny_read: Vec<PathBuf>,
    pub deny_write: Vec<PathBuf>,
    pub network_mode: crate::config::NetworkMode,
    pub backend_capabilities: crate::capability::BackendCapabilities,
    /// Backend expressiveness, not proof of successful kernel installation.
    pub enforcement_verified: bool,
    pub resolved_config: SandboxConfig,
    pub filesystem_policy: crate::path_policy::ResolvedPolicy,
    /// Exact compiler output, including masks, transient mounts and scoped grants.
    pub compiled_arguments: Vec<String>,
}

/// Validate and compile without spawning the payload or installing enforcement.
/// `config` is already merged with any workspace restrictions by its owner.
/// Inspection is currently supported only for the bwrap backend; other backends
/// fail rather than emit an incomplete effective-write report.
pub fn inspect_effective_policy(
    config: &SandboxConfig,
    workspace: &Path,
    cwd: &Path,
) -> Result<EffectivePolicyReport> {
    ensure!(
        cfg!(target_os = "linux"),
        "effective-policy reporting is not implemented for this backend"
    );
    ensure!(
        config.enabled,
        "effective-policy inspection requires sandboxing enabled"
    );
    let workspace = workspace
        .canonicalize()
        .context("resolving report workspace")?;
    let cwd = if cwd.is_absolute() {
        cwd.to_owned()
    } else {
        workspace.join(cwd)
    };
    let cwd = cwd.canonicalize().context("resolving report cwd")?;
    let command = build_sandbox_command_with_cwd(
        config,
        &workspace,
        &cwd,
        Path::new("/bin/true"),
        &[],
        SandboxStdin::Redirected,
    )?;
    let compiled_arguments: Vec<String> = command
        .get_args()
        .map(|arg| {
            arg.to_str()
                .map(str::to_owned)
                .context("compiled policy argument is not UTF-8")
        })
        .collect::<Result<_>>()?;
    let mut allow_write = Vec::new();
    for args in compiled_arguments.windows(3) {
        if (args[0] == "--bind" || args[0] == "--bind-try") && args[1] != "/dev/null" {
            allow_write.push(PathBuf::from(&args[1]));
            allow_write.push(PathBuf::from(&args[2]));
        }
    }
    allow_write.sort();
    allow_write.dedup();
    let build = config.permission_policy(&workspace, None)?;
    ensure!(
        build.unavailable_resources.is_empty(),
        "policy references unavailable resources"
    );
    Ok(EffectivePolicyReport {
        schema: "oqto.sandbox.effective.v1",
        profile: config.profile.clone(),
        workspace,
        cwd,
        allow_write,
        deny_read: build
            .policy
            .rules()
            .iter()
            .filter(|rule| rule.access == crate::path_policy::Access::None)
            .map(|rule| rule.path().to_owned())
            .collect(),
        deny_write: build
            .policy
            .rules()
            .iter()
            .filter(|rule| rule.access != crate::path_policy::Access::Write)
            .map(|rule| rule.path().to_owned())
            .collect(),
        network_mode: config.network_mode(),
        backend_capabilities: crate::capability::BackendCapabilities::bwrap(),
        enforcement_verified: false,
        resolved_config: config.clone(),
        filesystem_policy: build.policy,
        compiled_arguments,
    })
}
