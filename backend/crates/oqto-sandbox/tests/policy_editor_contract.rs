//! Integration tests through the public package contract.
use oqto_sandbox::{SandboxProfile, path_policy, policy_editor};

use anyhow::Result;
use path_policy::{Access, PolicyLayer, ResolvedPolicy, ResolvedRule, RuleOrigin};
use policy_editor::*;
use std::{fs, path::Path};

fn fixture(text: &str, scope: Scope) -> Result<(tempfile::TempDir, std::path::PathBuf, Proposal)> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("sandbox.toml");
    fs::write(&path, text)?;
    let snapshot = Snapshot::read(
        path.clone(),
        Binding {
            host: "fixture-host".into(),
            workspace: "fixture-workspace".into(),
        },
        scope,
    )?;
    assert_eq!(snapshot.text(), text);
    assert_eq!(snapshot.path(), path);
    assert_eq!(snapshot.scope(), scope);
    assert_eq!(snapshot.binding().host, "fixture-host");
    assert_ne!(snapshot.identity().fingerprint, 0);
    Ok((dir, path, Proposal::new(snapshot)))
}

#[test]
fn untouched_unknown_toml_is_preserved_and_rejected_not_dropped() -> Result<()> {
    let text = "# retain comment\nprofile = 'strict'\n[future_settings]\nunknown = [1, 2]\n";
    let (_dir, path, mut proposal) = fixture(text, Scope::Owner)?;
    let start = text
        .find("strict")
        .ok_or_else(|| anyhow::anyhow!("fixture"))?;
    proposal.replace(0, start..start + 6, "minimal".into())?;
    assert_eq!(proposal.candidate(), text.replacen("strict", "minimal", 1));
    assert_eq!(proposal.diff().map(|d| d.removed.as_str()), Some("strict"));
    assert!(proposal.validate().is_err());
    assert!(proposal.apply().is_err());
    proposal.cancel()?;
    assert!(proposal.validate().is_err());
    assert!(proposal.apply().is_err());
    assert_eq!(fs::read_to_string(path)?, text);
    Ok(())
}

#[test]
fn review_is_revision_bound_and_never_enables_apply() -> Result<()> {
    let text = "profile = 'strict'\n";
    let (_dir, path, mut proposal) = fixture(text, Scope::Owner)?;
    assert_eq!(proposal.validate()?, Validation::OwnerParsed);
    proposal.acknowledge_review(0)?;
    assert_eq!(proposal.reviewed_revision(), Some(0));
    proposal.replace(0, 0..0, "# proposed\n".into())?;
    assert_eq!(proposal.revision(), 1);
    assert_eq!(proposal.reviewed_revision(), None);
    assert!(proposal.acknowledge_review(0).is_err());
    assert!(proposal.replace(0, 0..0, String::new()).is_err());
    proposal.acknowledge_review(1)?;
    assert!(proposal.apply().is_err());
    assert_eq!(fs::read_to_string(&path)?, text);
    proposal.cancel()?;
    assert!(proposal.replace(2, 0..0, String::new()).is_err());
    assert_eq!(fs::read_to_string(path)?, text);
    Ok(())
}

#[test]
fn stale_contents_and_replaced_inode_fail() -> Result<()> {
    let text = "profile = 'strict'\n";
    let (dir, path, proposal) = fixture(text, Scope::Owner)?;
    fs::write(&path, "profile = 'minimal'\n")?;
    assert!(proposal.validate().is_err());
    let replacement = dir.path().join("replacement");
    fs::write(&replacement, text)?;
    fs::rename(replacement, &path)?;
    assert!(proposal.validate().is_err());
    Ok(())
}

#[test]
fn workspace_guard_widening_and_unknown_owner_profile_fail_closed() -> Result<()> {
    let (_dir, _path, proposal) = fixture("[guard]\nread = 'auto'\n", Scope::Workspace)?;
    assert!(proposal.validate().is_err());
    let (_dir, _path, proposal) = fixture("[guard]\nread = 'deny'\n", Scope::Workspace)?;
    assert_eq!(proposal.validate()?, Validation::WorkspaceResolved);
    let (_dir, _path, proposal) = fixture("profile = 'typo'\n", Scope::Owner)?;
    assert!(proposal.validate().is_err());
    Ok(())
}

#[test]
fn invalid_paths_and_symlink_ancestors_fail() -> Result<()> {
    assert!(inspect_path(Path::new("relative")).is_err());
    assert!(inspect_path(Path::new("/tmp/../etc")).is_err());
    assert!(inspect_path(Path::new("/tmp/./etc")).is_err());
    let (dir, path, mut proposal) = fixture("profile = 'strict'\n", Scope::Owner)?;
    assert!(proposal.replace(0, 1000..1001, String::new()).is_err());
    assert_eq!(proposal.revision(), 0);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&path, dir.path().join("link"))?;
        assert!(
            Snapshot::read(
                dir.path().join("link"),
                Binding {
                    host: "fixture".into(),
                    workspace: "fixture".into()
                },
                Scope::Owner
            )
            .is_err()
        );
        std::os::unix::fs::symlink(dir.path(), dir.path().join("parent-link"))?;
        assert!(inspect_path(&dir.path().join("parent-link/absent")).is_err());
    }
    assert_eq!(fs::read_to_string(path)?, "profile = 'strict'\n");
    Ok(())
}

#[test]
fn dependency_bytes_identity_and_binding_are_immutable_review_inputs() -> Result<()> {
    let (dir, target, _) = fixture("profile = 'strict'\n", Scope::Owner)?;
    let parent = dir.path().join("owner.toml");
    fs::write(&parent, "# owner revision one\nprofile = 'strict'\n")?;
    let binding = Binding {
        host: "fixture".into(),
        workspace: "fixture".into(),
    };
    let original = Snapshot::read(target.clone(), binding.clone(), Scope::Owner)?;
    let dependency = Snapshot::read(parent.clone(), binding.clone(), Scope::Owner)?;
    let mut proposal = Proposal::with_dependencies(original.clone(), vec![dependency.clone()])?;
    assert_eq!(proposal.snapshot().path(), target);
    assert_eq!(
        proposal.dependencies()[0].text(),
        fs::read_to_string(&parent)?
    );
    proposal.acknowledge_review(0)?;
    fs::write(&parent, "# owner revision two\nprofile = 'strict'\n")?;
    assert_eq!(proposal.reviewed_revision(), None);
    assert!(proposal.validate().is_err());
    assert!(proposal.acknowledge_review(0).is_err());
    assert!(proposal.apply().is_err());
    assert!(
        Proposal::with_dependencies(original.clone(), vec![dependency.clone(), dependency])
            .is_err()
    );
    assert!(Proposal::with_dependencies(original.clone(), vec![original.clone()]).is_err());
    let wrong = Snapshot::read(
        parent,
        Binding {
            host: "other-host".into(),
            workspace: binding.workspace,
        },
        Scope::Owner,
    )?;
    assert!(Proposal::with_dependencies(original, vec![wrong]).is_err());
    assert_eq!(fs::read_to_string(target)?, "profile = 'strict'\n");
    Ok(())
}

fn origin(layer: PolicyLayer) -> RuleOrigin {
    RuleOrigin {
        layer,
        source: "compiler fixture".into(),
    }
}

#[test]
fn compiler_layers_preserve_owner_ceiling_protected_locks_and_mixed_descendants() -> Result<()> {
    let dir = tempfile::tempdir()?;
    // Real translation + symbolic root compiler, not a second precedence algorithm.
    let profile = SandboxProfile::strict();
    let translated = oqto_sandbox::policy_translate::translate_profile("strict", &profile)?;
    let registry = path_policy::ResourceRegistry::default();
    let context = path_policy::ResolutionContext {
        workdir: dir.path(),
        home: &dir.path().join("home"),
        resources: &registry,
    };
    let (_config_dir, _config_path, proposal) = fixture("profile = 'strict'\n", Scope::Owner)?;
    let mut owner = proposal.compile_owner(&context)?.policy;
    let reference = translated.policy.resolve_roots(&context)?.policy;
    for rule in reference.rules() {
        assert_eq!(
            owner.resolve(rule.path())?.access,
            reference.resolve(rule.path())?.access
        );
    }
    owner.add_rule(ResolvedRule::new(
        dir.path(),
        Access::Read,
        origin(PolicyLayer::Admin),
    )?);
    let workspace = ResolvedPolicy::new(Access::Write, origin(PolicyLayer::Workspace));
    let mut protected = ResolvedPolicy::new(Access::Write, origin(PolicyLayer::System));
    let secret = dir.path().join("secret");
    protected.add_rule(ResolvedRule::new(
        &secret,
        Access::None,
        origin(PolicyLayer::System),
    )?);
    let root = effective_node(&owner, &workspace, &protected, dir.path())?;
    assert_eq!(root.path, dir.path());
    assert_eq!(root.effective.access, Access::Read);
    assert!(!root.inherited);
    assert!(root.mixed);
    assert!(root.contains_locked_descendant);
    let child = effective_node(&owner, &workspace, &protected, &dir.path().join("child"))?;
    assert!(child.inherited);
    assert_eq!(child.effective.access, Access::Read);
    let locked = effective_node(&owner, &workspace, &protected, &secret)?;
    assert!(locked.locked);
    assert!(!locked.contains_locked_descendant);
    assert_eq!(locked.effective.access, Access::None);
    assert_eq!(
        locked.effective,
        path_policy::resolve_layers(
            &[owner.clone(), workspace.clone(), protected.clone()],
            &secret
        )?
    );
    let mut proposed = owner.clone();
    proposed.add_rule(ResolvedRule::new(
        dir.path().join("child"),
        Access::Write,
        origin(PolicyLayer::Admin),
    )?);
    let changes = diff::containment_diff(&owner, &proposed, &workspace, &protected, dir.path())?;
    assert!(
        changes
            .iter()
            .any(|change| change.current.path == dir.path().join("child")
                && change.current.effective.access == Access::Read
                && change.proposed.effective.access == Access::Write)
    );
    assert!(!changes.iter().any(|change| change.proposed.path == secret));
    assert!(diff::containment_diff(&owner, &owner, &workspace, &protected, dir.path())?.is_empty());
    Ok(())
}
