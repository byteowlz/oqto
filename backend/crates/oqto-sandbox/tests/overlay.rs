#![cfg(target_os = "linux")]
use anyhow::Result;
use oqto_sandbox::{
    NetworkMode, SandboxConfig,
    overlay::{SandboxOverlay, load_private_overlay},
    path_policy::Access,
};
use std::os::unix::fs::PermissionsExt;

#[test]
fn replacement_settings_and_grants_are_not_overlay_fields() {
    for text in [
        "enabled = true",
        "profile = 'minimal'",
        "extra_ro_bind = ['/']",
        "extra_rw_bind = ['/']",
        "isolate_network = false",
        "landlock_mode = 'off'",
        "[network]\nmode = 'open'",
    ] {
        assert!(SandboxOverlay::parse(text).is_err(), "accepted: {text}");
    }
}

#[test]
fn overlay_retains_parent_denials_and_cannot_add_writes() -> Result<()> {
    let root = tempfile::tempdir()?;
    let work = root.path().join("work");
    std::fs::create_dir(&work)?;
    let mut parent = SandboxConfig::minimal();
    parent.allow_write = vec![root.path().join("authorized").display().to_string()];
    parent
        .deny_read
        .push(root.path().join("protected").display().to_string());
    let overlay = SandboxOverlay::parse("allow_write = []\n[network]\nmode = 'isolated'")?;
    let child = overlay.apply(&parent, &work)?;
    assert!(child.allow_write.is_empty());
    assert_eq!(child.deny_read, parent.deny_read);
    assert_eq!(child.extra_ro_bind, parent.extra_ro_bind);
    assert_eq!(child.extra_rw_bind, parent.extra_rw_bind);
    assert_eq!(child.network_mode(), NetworkMode::Isolated);
    assert!(
        SandboxOverlay::parse("allow_write = ['/']")?
            .apply(&parent, &work)
            .is_err()
    );
    Ok(())
}

#[test]
fn staging_only_write_summary_and_home_narrowing() -> Result<()> {
    let root = tempfile::tempdir()?;
    let work = root.path().join("work");
    std::fs::create_dir(&work)?;
    let parent = SandboxConfig::minimal();
    let child =
        SandboxOverlay::parse("home_access = 'none'\nallow_write = []")?.apply(&parent, &work)?;
    let report = oqto_sandbox::effective::inspect_effective_policy(
        &child,
        &work,
        std::path::Path::new("."),
    )?;
    assert_eq!(report.allow_write, vec![work.canonicalize()?]);
    assert!(
        SandboxOverlay::parse("home_access = 'write'")?
            .apply(&child, &work)
            .is_err()
    );
    Ok(())
}

#[test]
fn denies_are_compiled_and_repeat_narrowing_is_monotone() -> Result<()> {
    let root = tempfile::tempdir()?;
    let work = root.path().join("work");
    std::fs::create_dir(&work)?;
    let target = work.join("secret");
    std::fs::write(&target, "canary")?;
    let text = format!(
        "deny_read = [{}]",
        serde_json::to_string(&target.display().to_string())?
    );
    let overlay = SandboxOverlay::parse(&text)?;
    let parent = SandboxConfig::minimal();
    let child = overlay.apply(&parent, &work)?;
    let nested = overlay.apply(&child, &work)?;
    let report = oqto_sandbox::effective::inspect_effective_policy(
        &nested,
        &work,
        std::path::Path::new("."),
    )?;
    assert_eq!(
        report.filesystem_policy.resolve(&target)?.access,
        Access::None
    );
    assert!(
        report
            .compiled_arguments
            .windows(3)
            .any(|args| args[0] == "--bind"
                && args[1] == "/dev/null"
                && args[2] == target.display().to_string())
    );
    Ok(())
}

#[test]
fn rejects_ambiguous_paths_and_descendant_reopening() -> Result<()> {
    let root = tempfile::tempdir()?;
    let work = root.path().join("work");
    let nested = work.join("nested");
    std::fs::create_dir_all(&nested)?;
    let mut parent = SandboxConfig::minimal();
    parent.allow_write.push(nested.display().to_string());
    let overlay = SandboxOverlay::parse(&format!(
        "deny_read = [{}]",
        serde_json::to_string(&work.display().to_string())?
    ))?;
    assert!(overlay.apply(&parent, &work).is_err());
    for text in [
        "deny_read = ['relative']",
        "deny_write = ['/tmp/../etc']",
        "deny_read = ['/tmp/*']",
    ] {
        assert!(SandboxOverlay::parse(text)?.apply(&parent, &work).is_err());
    }
    Ok(())
}

#[test]
fn real_confined_child_and_nested_bind_cannot_recover_denied_data() -> Result<()> {
    let home = std::env::var("HOME")?;
    let root = tempfile::tempdir_in(home)?;
    let work = root.path().join("work");
    std::fs::create_dir(&work)?;
    let secret = work.join("secret");
    let readonly = work.join("readonly");
    std::fs::write(&secret, "overlay-secret-canary")?;
    std::fs::write(&readonly, "original")?;
    let overlay = SandboxOverlay::parse(&format!(
        "deny_read = [{}]\ndeny_write = [{}]",
        serde_json::to_string(&secret.display().to_string())?,
        serde_json::to_string(&readonly.display().to_string())?
    ))?;
    let config = overlay.apply(&SandboxConfig::minimal(), &work)?;
    let script = "test \"$(cat secret)\" != overlay-secret-canary && ! (echo changed > readonly) && bwrap --bind / / -- /bin/sh -c 'test \"$(cat secret)\" != overlay-secret-canary && ! (echo nested > readonly)'";
    let mut command = oqto_sandbox::build_sandbox_command(
        &config,
        &work,
        std::path::Path::new("/bin/sh"),
        &["-c".into(), script.into()],
        oqto_sandbox::SandboxStdin::Redirected,
    )?;
    oqto_sandbox::configure_bwrap_pre_exec(&mut command, &config, &work, None)?;
    let output = command.output()?;
    assert!(
        output.status.success(),
        "real confinement failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_to_string(&secret)?, "overlay-secret-canary");
    assert_eq!(std::fs::read_to_string(&readonly)?, "original");
    Ok(())
}

#[test]
fn isolated_overlay_blocks_host_loopback_listener() -> Result<()> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let root = tempfile::tempdir_in(std::env::var("HOME")?)?;
    let child = SandboxOverlay::parse("[network]\nmode = 'isolated'")?
        .apply(&SandboxConfig::minimal(), root.path())?;
    let script = format!("! (echo probe > /dev/tcp/127.0.0.1/{port})");
    let mut command = oqto_sandbox::build_sandbox_command(
        &child,
        root.path(),
        std::path::Path::new("/bin/bash"),
        &["-c".into(), script],
        oqto_sandbox::SandboxStdin::Redirected,
    )?;
    oqto_sandbox::configure_bwrap_pre_exec(&mut command, &child, root.path(), None)?;
    let output = command.output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    listener.set_nonblocking(true)?;
    assert!(listener.accept().is_err());
    Ok(())
}

#[test]
fn policy_capture_rejects_child_writable_placement_public_mode_and_symlink() -> Result<()> {
    let root = tempfile::tempdir()?;
    let work = root.path().join("work");
    std::fs::create_dir(&work)?;
    let policy = root.path().join("policy.toml");
    std::fs::write(&policy, "allow_write = []")?;
    std::fs::set_permissions(&policy, std::fs::Permissions::from_mode(0o600))?;
    load_private_overlay(&policy, &work)?;
    assert!(load_private_overlay(&policy, root.path()).is_err());
    let link = root.path().join("link.toml");
    std::os::unix::fs::symlink(&policy, &link)?;
    assert!(load_private_overlay(&link, &work).is_err());
    std::fs::set_permissions(&policy, std::fs::Permissions::from_mode(0o644))?;
    assert!(load_private_overlay(&policy, &work).is_err());
    Ok(())
}
