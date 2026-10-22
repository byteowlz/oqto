//! Strict file/text resolver equivalence, through real package exports.
use anyhow::{Context, Result};
use oqto_sandbox::{
    SandboxConfig, SandboxProfile,
    policy_editor::{Binding, Proposal, Scope, Snapshot, Validation},
};
use std::{collections::HashMap, fs};

#[test]
fn captured_workspace_text_matches_strict_file_resolution() -> Result<()> {
    let dir = tempfile::tempdir()?;
    fs::create_dir(dir.path().join(".oqto"))?;
    let path = dir.path().join(".oqto/sandbox.toml");
    let mut global = HashMap::new();
    let mut custom = SandboxProfile::strict();
    custom.allow_read.push("~/global-profile-marker".into());
    global.insert("owner-custom".into(), custom);
    let cases = [
        "profile = 'strict'\n[guard]\nread = 'deny'\n",
        "profile = 'owner-custom'\n",
        "profile = 'local'\n[profiles.local]\nallow_read = ['~/local-marker']\n",
        "profile = 'owner-custom'\n[profiles.owner-custom]\nallow_read = ['~/overrides-global']\n",
        "profile = 'unknown-profile'\n",
        "[guard]\nread = 'auto'\n",
        "[guard]\nauto_shell = ['git status']\n",
        "[guard\n",
    ];
    for text in cases {
        fs::write(&path, text)?;
        let loaded = SandboxConfig::try_load_from_workspace(dir.path(), &global);
        let resolved = SandboxConfig::resolve_workspace_text(text, &path, &global);
        let snapshot = Snapshot::read(
            path.clone(),
            Binding {
                host: "fixture".into(),
                workspace: "fixture".into(),
            },
            Scope::Workspace,
        )?;
        let proposal = Proposal::new(snapshot);
        let proposed = proposal.resolve_workspace(&global);
        match (loaded, resolved, proposed) {
            (Ok(Some(file)), Ok(captured), Ok(candidate)) => {
                assert_eq!(
                    serde_json::to_value(&file)?,
                    serde_json::to_value(&captured)?,
                    "{text}"
                );
                assert_eq!(
                    serde_json::to_value(file)?,
                    serde_json::to_value(candidate)?,
                    "{text}"
                );
                assert_eq!(
                    proposal.validate_with_profiles(&global)?,
                    Validation::WorkspaceResolved
                );
            }
            (Err(file), Err(captured), Err(candidate)) => {
                assert_eq!(format!("{file:#}"), format!("{captured:#}"), "{text}");
                assert_eq!(format!("{file:#}"), format!("{candidate:#}"), "{text}");
                assert!(proposal.validate_with_profiles(&global).is_err());
            }
            _ => anyhow::bail!("file/text/proposal resolution diverged for {text}"),
        }
        assert_eq!(fs::read_to_string(&path)?, text);
    }
    fs::remove_file(&path)?;
    assert!(SandboxConfig::try_load_from_workspace(dir.path(), &global)?.is_none());
    // Diagnostic source is not an I/O target. Text resolution still succeeds.
    let config = SandboxConfig::resolve_workspace_text("profile = 'owner-custom'", &path, &global)
        .context("pure resolver with absent diagnostic path")?;
    assert!(
        config
            .allow_read
            .iter()
            .any(|p| p == "~/global-profile-marker")
    );
    Ok(())
}
