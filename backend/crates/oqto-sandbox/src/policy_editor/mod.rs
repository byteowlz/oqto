//! Read-only, revisioned policy proposals. This module confers no authority.
//!
//! Review is presentation state, not authorization. Persistent apply is unavailable.
//! Callers supply compiler-produced layers; their provenance is not authenticated here.

use crate::{
    SandboxConfigFile, SandboxProfile,
    path_policy::{Access, LayeredResolution, ResolvedPolicy, resolve_layers},
};
pub mod diff;

use anyhow::{Context, Result, bail, ensure};
use std::{
    fs,
    ops::Range,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub host: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Owner,
    Workspace,
}

/// Exact bytes are the concurrency authority, not this diagnostic FNV fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIdentity {
    pub fingerprint: u64,
    #[cfg(unix)]
    pub device: u64,
    #[cfg(unix)]
    pub inode: u64,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    path: PathBuf,
    binding: Binding,
    scope: Scope,
    identity: FileIdentity,
    text: String,
}

/// Reject traversal and symlinks in every existing component. This is inspection,
/// not a race-free publication primitive; a writer must use directory handles.
pub fn inspect_path(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "absolute path required");
    // components() normalizes `.`; reject it lexically too.
    ensure!(
        !path
            .as_os_str()
            .to_string_lossy()
            .split('/')
            .any(|p| p == "." || p == ".."),
        "non-normal path"
    );
    let mut prefix = PathBuf::new();
    for component in path.components() {
        ensure!(
            !matches!(component, Component::ParentDir | Component::CurDir),
            "non-normal path"
        );
        prefix.push(component);
        match fs::symlink_metadata(&prefix) {
            Ok(meta) => ensure!(!meta.file_type().is_symlink(), "symlink path component"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("inspect path"),
        }
    }
    Ok(())
}

fn capture(path: &Path) -> Result<(FileIdentity, String)> {
    inspect_path(path)?;
    let meta = fs::symlink_metadata(path).context("config metadata")?;
    ensure!(meta.is_file(), "config must be a regular file");
    let text = fs::read_to_string(path).context("read config")?;
    inspect_path(path)?;
    let after = fs::symlink_metadata(path).context("recheck config metadata")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            meta.dev() == after.dev() && meta.ino() == after.ino(),
            "config identity changed during capture"
        );
    }
    ensure!(
        after.is_file() && meta.len() == after.len() && meta.modified()? == after.modified()?,
        "config changed during capture"
    );
    let fingerprint = text.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    Ok((
        FileIdentity {
            fingerprint,
            #[cfg(unix)]
            device: {
                use std::os::unix::fs::MetadataExt;
                meta.dev()
            },
            #[cfg(unix)]
            inode: {
                use std::os::unix::fs::MetadataExt;
                meta.ino()
            },
        },
        text,
    ))
}

impl Snapshot {
    pub fn read(path: PathBuf, binding: Binding, scope: Scope) -> Result<Self> {
        ensure!(
            !binding.host.is_empty() && !binding.workspace.is_empty(),
            "empty binding"
        );
        let (identity, text) = capture(&path)?;
        Ok(Self {
            path,
            binding,
            scope,
            identity,
            text,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn scope(&self) -> Scope {
        self.scope
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn identity(&self) -> &FileIdentity {
        &self.identity
    }
    pub fn binding(&self) -> &Binding {
        &self.binding
    }
    pub fn revalidate(&self) -> Result<()> {
        let (identity, text) = capture(&self.path)?;
        ensure!(
            identity == self.identity && text == self.text,
            "stale config snapshot"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDiff {
    pub removed: String,
    pub inserted: String,
    pub range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Validation {
    /// Real owner config conversion has run; no authorization is implied.
    OwnerParsed,
    /// Canonical strict workspace resolution completed. Guard approval evaluation
    /// and owner/workspace containment composition remain higher-layer concerns.
    WorkspaceResolved,
}

#[derive(Debug)]
pub struct Proposal {
    snapshot: Snapshot,
    dependencies: Vec<Snapshot>,
    revision: u64,
    candidate: String,
    diff: Option<TextDiff>,
    reviewed_revision: Option<u64>,
    cancelled: bool,
}

impl Proposal {
    pub fn new(snapshot: Snapshot) -> Self {
        Self {
            candidate: snapshot.text.clone(),
            snapshot,
            dependencies: Vec::new(),
            revision: 0,
            diff: None,
            reviewed_revision: None,
            cancelled: false,
        }
    }
    /// The trusted host must supply ALL effective config dependencies. This
    /// constructor pins supplied inputs but cannot certify their completeness.
    pub fn with_dependencies(snapshot: Snapshot, mut dependencies: Vec<Snapshot>) -> Result<Self> {
        dependencies.sort_by(|a, b| a.path.cmp(&b.path));
        for (index, dependency) in dependencies.iter().enumerate() {
            ensure!(
                dependency.binding == snapshot.binding,
                "dependency binding mismatch"
            );
            ensure!(
                dependency.path != snapshot.path,
                "target cannot be its own dependency"
            );
            if index > 0 {
                ensure!(
                    dependencies[index - 1].path != dependency.path,
                    "duplicate dependency"
                );
            }
            dependency.revalidate()?;
        }
        snapshot.revalidate()?;
        let mut proposal = Self::new(snapshot);
        proposal.dependencies = dependencies;
        Ok(proposal)
    }
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn dependencies(&self) -> &[Snapshot] {
        &self.dependencies
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn candidate(&self) -> &str {
        &self.candidate
    }
    pub fn diff(&self) -> Option<&TextDiff> {
        self.diff.as_ref()
    }
    pub fn reviewed_revision(&self) -> Option<u64> {
        self.reviewed_revision.filter(|_| self.validate().is_ok())
    }
    /// Replacement always addresses the original snapshot, not prior proposals.
    /// Unchanged ranges are copied byte-for-byte, including unknown TOML/comments.
    pub fn replace(
        &mut self,
        expected_revision: u64,
        range: Range<usize>,
        inserted: String,
    ) -> Result<()> {
        ensure!(
            !self.cancelled && expected_revision == self.revision,
            "stale or cancelled proposal"
        );
        let original = &self.snapshot.text;
        let removed = original
            .get(range.clone())
            .context("invalid UTF-8 edit range")?
            .to_owned();
        let next = self.revision.checked_add(1).context("revision exhausted")?;
        let mut candidate = original.clone();
        candidate.replace_range(range.clone(), &inserted);
        self.candidate = candidate;
        self.diff = Some(TextDiff {
            removed,
            inserted,
            range,
        });
        self.revision = next;
        self.reviewed_revision = None;
        Ok(())
    }
    pub fn validate(&self) -> Result<Validation> {
        self.validate_with_profiles(&std::collections::HashMap::new())
    }
    fn revalidate_inputs(&self) -> Result<()> {
        ensure!(!self.cancelled, "cancelled proposal");
        self.snapshot.revalidate()?;
        for dependency in &self.dependencies {
            dependency.revalidate()?;
        }
        Ok(())
    }
    /// Resolve a workspace candidate without writing scratch files. Profiles must
    /// come from trusted, snapshot-pinned owner inputs, not UI authority claims.
    pub fn resolve_workspace(
        &self,
        global_profiles: &std::collections::HashMap<String, SandboxProfile>,
    ) -> Result<crate::SandboxConfig> {
        self.revalidate_inputs()?;
        ensure!(
            self.snapshot.scope == Scope::Workspace,
            "workspace proposal required"
        );
        crate::SandboxConfig::resolve_workspace_text(
            &self.candidate,
            &self.snapshot.path,
            global_profiles,
        )
    }
    pub fn validate_with_profiles(
        &self,
        global_profiles: &std::collections::HashMap<String, SandboxProfile>,
    ) -> Result<Validation> {
        self.revalidate_inputs()?;
        if self.snapshot.scope == Scope::Workspace {
            self.resolve_workspace(global_profiles)?;
            return Ok(Validation::WorkspaceResolved);
        }
        let file: SandboxConfigFile =
            toml::from_str(&self.candidate).context("canonical sandbox config parser")?;
        ensure!(
            file.profile.is_empty()
                || file.profiles.contains_key(&file.profile)
                || SandboxProfile::builtin(&file.profile).is_some(),
            "unknown profile"
        );
        let _: crate::SandboxConfig = file.into();
        Ok(Validation::OwnerParsed)
    }
    /// Compile owner containment with the existing translation and root resolver.
    /// Guard approval semantics are separate and are not represented by this tree.
    pub fn compile_owner(
        &self,
        context: &crate::path_policy::ResolutionContext<'_>,
    ) -> Result<crate::path_policy::ResolvedPolicyBuild> {
        ensure!(
            self.snapshot.scope == Scope::Owner,
            "workspace compilation requires strict loader integration"
        );
        self.validate()?;
        let file: SandboxConfigFile = toml::from_str(&self.candidate)?;
        let config: crate::SandboxConfig = file.into();
        let translated = crate::policy_translate::translate_permissions(
            &crate::policy_translate::PermissionFields {
                profile_name: &config.profile,
                read_policy: config.read_policy,
                home_access: config.home_access,
                allow_read: &config.allow_read,
                deny_read: &config.deny_read,
                allow_write: &config.allow_write,
                deny_write: &config.deny_write,
                extra_ro_bind: &config.extra_ro_bind,
                extra_rw_bind: &config.extra_rw_bind,
            },
        )?;
        Ok(translated.policy.resolve_roots(context)?)
    }

    /// Records that a presentation showed this revision. Never an operator receipt.
    pub fn acknowledge_review(&mut self, expected_revision: u64) -> Result<()> {
        ensure!(expected_revision == self.revision, "stale review");
        self.validate()?;
        self.reviewed_revision = Some(self.revision);
        Ok(())
    }
    pub fn cancel(&mut self) -> Result<()> {
        self.revision = self.revision.checked_add(1).context("revision exhausted")?;
        self.cancelled = true;
        self.reviewed_revision = None;
        Ok(())
    }
    pub fn apply(&self) -> Result<()> {
        bail!(
            "persistent apply unavailable: no authenticated operator/protected writer adapter; same-UID runner is not authority"
        )
    }
}

#[derive(Debug, Clone)]
pub struct EffectiveNode {
    pub path: PathBuf,
    pub effective: LayeredResolution,
    pub inherited: bool,
    /// Exact over the supplied compiler rule boundaries, not enumerated files.
    pub mixed: bool,
    pub locked: bool,
    /// A blanket subtree grant would contain a protected denial.
    pub contains_locked_descendant: bool,
}

/// The actual resolver owns all precedence. Protected policy is a mandatory layer,
/// not a switch a client may use to unlock a node. Provenance is the integrator's duty.
pub fn effective_node(
    owner: &ResolvedPolicy,
    workspace: &ResolvedPolicy,
    protected: &ResolvedPolicy,
    path: &Path,
) -> Result<EffectiveNode> {
    inspect_path(path)?;
    let layers = [owner.clone(), workspace.clone(), protected.clone()];
    let effective = resolve_layers(&layers, path)?;
    let inherited = !effective
        .decisions
        .iter()
        .any(|d| d.matched_path.as_deref() == Some(path));
    let mut mixed = false;
    for rule in layers.iter().flat_map(ResolvedPolicy::rules) {
        if rule.path() != path
            && rule.path().starts_with(path)
            && resolve_layers(&layers, rule.path())?.access != effective.access
        {
            mixed = true;
        }
    }
    let locked = protected.resolve(path)?.access == Access::None;
    let mut contains_locked_descendant = false;
    for rule in protected.rules() {
        if rule.path() != path
            && rule.path().starts_with(path)
            && protected.resolve(rule.path())?.access == Access::None
        {
            contains_locked_descendant = true;
        }
    }
    Ok(EffectiveNode {
        path: path.to_owned(),
        effective,
        inherited,
        mixed,
        locked,
        contains_locked_descendant,
    })
}
