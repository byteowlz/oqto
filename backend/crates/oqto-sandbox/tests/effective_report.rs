#![cfg(target_os = "linux")]
use oqto_sandbox::{SandboxConfig, effective::inspect_effective_policy};
use std::path::Path;

#[test]
fn report_includes_implicit_workspace_write_and_does_not_spawn() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let workspace = directory.path().canonicalize()?;
    let config = SandboxConfig::minimal();
    let report = inspect_effective_policy(&config, &workspace, Path::new("."))?;
    assert_eq!(report.schema, "oqto.sandbox.effective.v1");
    assert!(!report.enforcement_verified);
    assert!(report.allow_write.contains(&workspace));
    assert_eq!(std::fs::read_dir(&workspace)?.count(), 0);
    let json = serde_json::to_value(&report)?;
    assert!(json["filesystem_policy"]["rules"].is_array());
    assert_eq!(json["backend_capabilities"]["backend"], "bwrap");
    Ok(())
}

#[test]
fn changing_cwd_does_not_change_write_reach() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    std::fs::create_dir(directory.path().join("nested"))?;
    let config = SandboxConfig::minimal();
    let root = inspect_effective_policy(&config, directory.path(), Path::new("."))?;
    let nested = inspect_effective_policy(&config, directory.path(), Path::new("nested"))?;
    assert_eq!(root.allow_write, nested.allow_write);
    assert_ne!(root.cwd, nested.cwd);
    assert!(inspect_effective_policy(&config, directory.path(), Path::new("..")).is_err());
    Ok(())
}

#[test]
fn disabled_policy_cannot_produce_successful_validation() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let mut config = SandboxConfig::minimal();
    config.enabled = false;
    assert!(inspect_effective_policy(&config, directory.path(), Path::new(".")).is_err());
    Ok(())
}
