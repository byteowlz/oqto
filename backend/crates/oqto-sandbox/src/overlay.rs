//! Restriction-only input: never an alternative source of policy authority.
use crate::path_policy::Access;
use crate::{NetworkConfig, NetworkMode, SandboxConfig};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct SandboxOverlay {
    pub deny_read: Vec<String>,
    pub deny_write: Vec<String>,
    /// Exact retained entries only. No paths, binds or implicit workspace grants
    /// are added by this field. An empty list removes explicit allow_write entries.
    pub allow_write: Option<Vec<String>>,
    pub home_access: Option<crate::config::HomeAccess>,
    pub network: Option<OverlayNetwork>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverlayNetwork {
    pub mode: NetworkMode,
}

/// Capture bounded private TOML before launching. The policy file itself must
/// not reside in the workspace grant. Same-UID protection still requires a
/// trusted owner/runner and confinement; mode bits alone are not that protection.
pub fn load_private_overlay(path: &Path, workspace: &Path) -> Result<SandboxOverlay> {
    use std::io::Read;
    #[cfg(unix)]
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let canonical = path.canonicalize().context("resolving overlay file")?;
    let workspace = workspace
        .canonicalize()
        .context("resolving overlay workspace")?;
    ensure!(
        !canonical.starts_with(&workspace),
        "overlay must be outside the workspace grant"
    );
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    let file = options.open(path).context("opening private overlay")?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= 65536,
        "overlay must be a regular file of at most 64 KiB"
    );
    #[cfg(unix)]
    ensure!(
        metadata.mode() & 0o077 == 0,
        "overlay must not be accessible to group or others"
    );
    let mut text = String::new();
    file.take(65537)
        .read_to_string(&mut text)
        .context("reading overlay snapshot")?;
    ensure!(text.len() <= 65536, "overlay exceeds 64 KiB");
    SandboxOverlay::parse(&text)
}

impl SandboxOverlay {
    pub fn parse(text: &str) -> Result<Self> {
        let overlay: Self =
            toml::from_str(text).context("parsing restriction-only sandbox overlay")?;
        if let Some(network) = &overlay.network {
            ensure!(
                network.mode == NetworkMode::Isolated,
                "overlay network must be isolated; opening or replacing egress is not permitted"
            );
        }
        Ok(overlay)
    }

    /// Applies restrictions to a trusted config; rejects attempted additions.
    /// This does not authenticate the caller or establish a parent Session ceiling.
    pub fn apply(&self, parent: &SandboxConfig, workspace: &Path) -> Result<SandboxConfig> {
        ensure!(parent.enabled, "overlay cannot enable a disabled sandbox");
        if let Some(network) = &self.network {
            ensure!(
                network.mode == NetworkMode::Isolated,
                "overlay network must be isolated"
            );
        }
        for path in self.deny_read.iter().chain(&self.deny_write) {
            ensure!(
                !path.is_empty() && (Path::new(path).is_absolute() || path.starts_with("~/")),
                "overlay deny paths must be absolute or home-relative"
            );
            ensure!(
                !path.contains('*')
                    && !Path::new(path)
                        .components()
                        .any(|part| part == std::path::Component::ParentDir),
                "overlay deny paths cannot contain globs or parent traversal"
            );
        }
        let mut child = parent.clone();
        if let Some(access) = self.home_access {
            ensure!(
                access <= parent.home_access,
                "overlay cannot widen home access"
            );
            child.home_access = access;
        }
        if let Some(retain) = &self.allow_write {
            ensure!(
                retain.iter().all(|path| parent.allow_write.contains(path)),
                "overlay allow_write may only retain exact existing entries, not add grants"
            );
            child.allow_write.retain(|path| retain.contains(path));
        }
        child.deny_read.extend(self.deny_read.iter().cloned());
        child.deny_write.extend(self.deny_write.iter().cloned());
        if self.network.is_some() {
            child.network = Some(NetworkConfig {
                mode: NetworkMode::Isolated,
                ..NetworkConfig::default()
            });
            child.isolate_network = true;
        }
        let before = parent.permission_policy(workspace, None)?;
        let after = child.permission_policy(workspace, None)?;
        ensure!(
            before.unavailable_resources.is_empty() && after.unavailable_resources.is_empty(),
            "overlay policy references unavailable resources"
        );
        // Every decision breakpoint of either finite path tree is checked. Merely
        // intersecting strings does not establish containment across nested roots.
        for rule in before.policy.rules().iter().chain(after.policy.rules()) {
            let old = before.policy.resolve(rule.path())?;
            let new = after.policy.resolve(rule.path())?;
            ensure!(
                new.access <= old.access,
                "overlay widens filesystem policy at {:?}",
                rule.path()
            );
        }
        ensure!(
            after.policy.default <= before.policy.default,
            "overlay widens default access"
        );
        // Deny overlays must not silently retain descendant exceptions.
        for rule in after.policy.rules() {
            if rule.origin.source.contains("deny_read") && rule.access == Access::None {
                ensure!(
                    !after
                        .policy
                        .rules()
                        .iter()
                        .any(|child_rule| child_rule.path() != rule.path()
                            && child_rule.path().starts_with(rule.path())
                            && child_rule.access > Access::None),
                    "deny_read has descendant exceptions; refusing incomplete denial at {:?}",
                    rule.path()
                );
            }
        }
        let report = crate::effective::inspect_effective_policy(&child, workspace, Path::new("."))?;
        for rule in after.policy.rules() {
            let read_denial =
                rule.origin.source.contains("deny_read") && rule.access == Access::None;
            let write_denial =
                rule.origin.source.contains("deny_write") && rule.access < Access::Write;
            if !read_denial && !write_denial {
                continue;
            }
            for mount in report.compiled_arguments.windows(3) {
                let grants_read = mount[0] == "--bind" || mount[0] == "--ro-bind";
                let grants_write = mount[0] == "--bind";
                if mount[1] != "/dev/null"
                    && (Path::new(&mount[2]).starts_with(rule.path())
                        || Path::new(&mount[1]).starts_with(rule.path())
                        || (mount[1] != mount[2] && rule.path().starts_with(Path::new(&mount[1]))))
                {
                    ensure!(
                        !(read_denial && grants_read || write_denial && grants_write),
                        "compiled mount conflicts with overlay denial at {:?}",
                        rule.path()
                    );
                }
            }
        }
        Ok(child)
    }
}
