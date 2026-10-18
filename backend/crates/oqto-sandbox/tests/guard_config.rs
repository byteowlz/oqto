//! ADR-0054: runtime approvals are the oqto-guard layer of the one sandbox config.

use std::fs;

use oqto_sandbox::{GuardConfig, GuardPolicy, SandboxConfig, SandboxConfigFile};
use tempfile::TempDir;

const USER: &str = r#"
profile = "development"

[profiles.guarded.guard]
paths = ["~/Code"]
edit = "auto"

[guard]
read = "auto"
shell = "prompt"
outside = "deny"
auto_shell = ["cargo test"]
paths = ["~/Documents/shared"]

[guard.policy]
"~/Code/secrets/**" = "deny"
"~/Code/library" = "auto"
"#;

#[test]
fn user_file_guard_overrides_profile_guard_and_carries_through() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("sandbox.toml");
    fs::write(
        &file,
        USER.replace("profile = \"development\"", "profile = \"guarded\""),
    )
    .unwrap();
    let config = SandboxConfig::load_user_config_at(&file).unwrap();
    let guard = config.guard.expect("resolved guard");
    // Profile values survive unless overridden; lists union; scalars replace.
    assert_eq!(guard.edit, Some(GuardPolicy::Auto), "profile guard kept");
    assert_eq!(guard.read, Some(GuardPolicy::Auto));
    assert_eq!(guard.shell, Some(GuardPolicy::Prompt));
    assert_eq!(guard.outside, Some(GuardPolicy::Deny));
    assert_eq!(guard.auto_shell, vec!["cargo test".to_string()]);
    assert!(guard.paths.contains(&"~/Code".to_string()));
    assert!(guard.paths.contains(&"~/Documents/shared".to_string()));
    assert_eq!(
        guard.policy.get("~/Code/secrets/**"),
        Some(&GuardPolicy::Deny)
    );
    assert!(config.workspace_guard.is_none());
}

#[test]
fn guard_only_in_user_file_without_profile_guard() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("sandbox.toml");
    fs::write(&file, USER).unwrap();
    let config = SandboxConfig::load_user_config_at(&file).unwrap();
    assert_eq!(config.guard.unwrap().shell, Some(GuardPolicy::Prompt));
}

#[test]
fn user_file_missing_is_default_and_unknown_profile_fails_closed() {
    let dir = TempDir::new().unwrap();
    let config = SandboxConfig::load_user_config_at(&dir.path().join("absent.toml")).unwrap();
    assert!(config.guard.is_none());
    let file = dir.path().join("sandbox.toml");
    fs::write(&file, "profile = \"no-such-profile\"\n").unwrap();
    let error = SandboxConfig::load_user_config_at(&file)
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown profile"), "{error}");
    fs::write(&file, "[guard]\nedit = \"sometimes\"\n").unwrap();
    assert!(SandboxConfig::load_user_config_at(&file).is_err());
}

fn workspace(content: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(".oqto")).unwrap();
    fs::write(dir.path().join(".oqto/sandbox.toml"), content).unwrap();
    dir
}

#[test]
fn workspace_guard_is_a_separate_narrowing_layer() {
    let ws = workspace(
        "[guard]\nedit = \"deny\"\noutside = \"deny\"\n[guard.policy]\n\"~/Code/x\" = \"prompt\"\n",
    );
    let global = SandboxConfig::from_profile("strict");
    let loaded = SandboxConfig::try_load_from_workspace(ws.path(), &global.profiles)
        .unwrap()
        .unwrap();
    let merged = global.merge_with_workspace(&loaded);
    let layer = merged.workspace_guard.expect("workspace layer kept apart");
    assert_eq!(layer.edit, Some(GuardPolicy::Deny));
    assert_eq!(
        merged.guard, global.guard,
        "owner guard is never merged with the workspace's"
    );
}

#[test]
fn workspace_guard_may_never_pre_approve() {
    for bad in [
        "[guard]\nauto_shell = [\"npm test\"]\n",
        "[guard]\nedit = \"auto\"\n",
        "[guard]\noutside = \"auto\"\n",
        "[guard.policy]\n\"~/Code\" = \"auto\"\n",
    ] {
        let ws = workspace(bad);
        let error = SandboxConfig::try_load_from_workspace(ws.path(), &Default::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("guard"), "{bad}: {error}");
        // The lenient runner loader keeps its old behaviour (warn + ignore).
        assert!(SandboxConfig::load_from_workspace(ws.path(), &Default::default()).is_none());
    }
}

#[test]
fn strict_workspace_loader_fails_closed_lenient_one_does_not() {
    let ws = workspace("this is = = not toml");
    assert!(SandboxConfig::try_load_from_workspace(ws.path(), &Default::default()).is_err());
    assert!(SandboxConfig::load_from_workspace(ws.path(), &Default::default()).is_none());
    let ws = workspace("profile = \"no-such-profile\"\n");
    assert!(SandboxConfig::try_load_from_workspace(ws.path(), &Default::default()).is_err());
    let empty = TempDir::new().unwrap();
    assert!(
        SandboxConfig::try_load_from_workspace(empty.path(), &Default::default())
            .unwrap()
            .is_none()
    );
}

#[test]
fn guard_overlay_and_schema_carry_the_new_fields() {
    let profile = GuardConfig {
        edit: Some(GuardPolicy::Prompt),
        auto_shell: vec!["cargo test".into()],
        ..Default::default()
    };
    let owner = GuardConfig {
        edit: Some(GuardPolicy::Auto),
        auto_shell: vec!["npm test".into(), "cargo test".into()],
        ..Default::default()
    };
    let merged = profile.overlay(&owner);
    assert_eq!(merged.edit, Some(GuardPolicy::Auto));
    assert_eq!(
        merged.auto_shell,
        vec!["cargo test".to_string(), "npm test".to_string()]
    );
    let schema = SandboxConfigFile::schema_json().expect("schema serializes");
    assert!(
        schema.contains("\"auto_shell\""),
        "schema exposes guard.auto_shell"
    );
    assert!(schema.contains("\"outside\""));
}
