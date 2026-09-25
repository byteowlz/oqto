//! Transactional release activation through the real oqto-setup CLI.
//! All installation paths are redirected into a temporary directory.

use anyhow::{Result, ensure};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

fn bundle(root: &Path, id: &str) -> Result<std::path::PathBuf> {
    bundle_with_layout(root, id, true)
}

fn bundle_with_layout(root: &Path, id: &str, complete: bool) -> Result<std::path::PathBuf> {
    let name = format!("oqto-{id}-x86_64-unknown-linux-gnu");
    let stage_root = root.join("stage");
    let staging = stage_root.join(&name);
    fs::create_dir_all(staging.join("immutable/bin"))?;
    let names: &[&str] = if complete {
        &[
            "oqto",
            "oqtoctl",
            "oqto-setup",
            "oqto-runner",
            "oqto-files",
            "oqto-sandbox",
            "oqto-usermgr",
            "pi-bridge",
        ]
    } else {
        &["oqto"]
    };
    for name in names {
        let binary = staging.join("immutable/bin").join(name);
        fs::write(&binary, format!("#!/bin/sh\nprintf '%s\\n' '{id}'\n"))?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))?;
    }
    if complete {
        fs::write(
            staging.join("manifest.toml"),
            "manifest_version = 1\nid = \"oqto-dist\"\n[release]\ntarget = \"full\"\n",
        )?;
    }
    let artifact = root.join(format!("{name}.tar.gz"));
    let status = Command::new("tar")
        .arg("-C")
        .arg(&stage_root)
        .arg("-czf")
        .arg(&artifact)
        .arg(&name)
        .status()?;
    ensure!(status.success(), "tar failed creating test artifact");
    Ok(artifact)
}

fn install(
    artifact: &Path,
    releases: &Path,
    binaries: &Path,
    doctor: &Path,
    strict: bool,
) -> Result<std::process::Output> {
    let output = Command::new(env!("CARGO_BIN_EXE_oqto-setup"))
        .arg("install")
        .arg("--artifact")
        .arg(artifact)
        .arg("--releases-root")
        .arg(releases)
        .arg("--bin-dir")
        .arg(binaries)
        .args(["--doctor-strict", if strict { "true" } else { "false" }])
        .env(
            "PATH",
            format!("{}:{}", doctor.display(), std::env::var("PATH")?),
        )
        .output()?;
    Ok(output)
}

fn rejecting_doctor(root: &Path) -> Result<std::path::PathBuf> {
    let dir = root.join("fake-doctor");
    fs::create_dir_all(&dir)?;
    let path = dir.join("oqtoctl");
    fs::write(&path, "#!/bin/sh\nexit 1\n")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(dir)
}

#[test]
fn incomplete_bundle_cannot_activate_even_with_doctor_deferred() -> Result<()> {
    let root = tempfile::tempdir()?;
    let releases = root.path().join("releases");
    let binaries = root.path().join("bin");
    // A runner-only candidate or an arbitrary tar with just one executable
    // must not masquerade as a full installation when doctor is deferred.
    let artifact = bundle_with_layout(root.path(), "test-incomplete", false)?;
    let doctor = rejecting_doctor(root.path())?;
    let output = install(&artifact, &releases, &binaries, &doctor, false)?;
    ensure!(
        !output.status.success(),
        "incomplete artifact was activated"
    );
    ensure!(fs::symlink_metadata(releases.join("current")).is_err());
    ensure!(fs::symlink_metadata(binaries.join("oqto")).is_err());
    Ok(())
}

#[test]
fn failed_fresh_install_does_not_leave_current_or_entrypoints() -> Result<()> {
    let root = tempfile::tempdir()?;
    let releases = root.path().join("releases");
    let binaries = root.path().join("bin");
    let artifact = bundle(root.path(), "test-fresh")?;
    let doctor = rejecting_doctor(root.path())?;

    let output = install(&artifact, &releases, &binaries, &doctor, true)?;
    ensure!(
        !output.status.success(),
        "failing doctor must reject install"
    );
    ensure!(
        fs::symlink_metadata(releases.join("current")).is_err(),
        "failed fresh activation left a current pointer"
    );
    ensure!(
        fs::symlink_metadata(releases.join("last-good")).is_err(),
        "failed fresh activation advanced last-good"
    );
    ensure!(
        fs::symlink_metadata(binaries.join("oqto")).is_err(),
        "failed fresh activation left an oqto entrypoint"
    );
    ensure!(
        !releases
            .join("oqto-test-fresh-x86_64-unknown-linux-gnu")
            .exists(),
        "failed fresh activation left an unreferenced staged release"
    );
    Ok(())
}

#[test]
fn fresh_install_refuses_to_replace_an_unmanaged_entrypoint() -> Result<()> {
    let root = tempfile::tempdir()?;
    let releases = root.path().join("releases");
    let binaries = root.path().join("bin");
    fs::create_dir_all(&binaries)?;
    let existing = binaries.join("oqto");
    fs::write(&existing, b"user-owned binary; do not replace")?;
    let artifact = bundle(root.path(), "test-existing")?;
    let doctor = rejecting_doctor(root.path())?;

    let output = install(&artifact, &releases, &binaries, &doctor, false)?;
    ensure!(
        !output.status.success(),
        "install replaced an unmanaged executable"
    );
    ensure!(
        fs::read(&existing)? == b"user-owned binary; do not replace",
        "unmanaged executable was changed"
    );
    ensure!(
        fs::symlink_metadata(releases.join("current")).is_err(),
        "unmanaged entrypoint conflict switched current"
    );
    Ok(())
}

#[test]
fn failed_upgrade_restores_previous_release_and_managed_entrypoint() -> Result<()> {
    let root = tempfile::tempdir()?;
    let releases = root.path().join("releases");
    let binaries = root.path().join("bin");
    let old = bundle(root.path(), "test-old")?;
    let new = bundle(root.path(), "test-new")?;
    let doctor = rejecting_doctor(root.path())?;

    let first = install(&old, &releases, &binaries, &doctor, false)?;
    ensure!(first.status.success(), "old release could not be activated");
    let previous = fs::read_link(releases.join("current"))?;
    ensure!(fs::read_link(releases.join("last-good"))? == previous);

    let second = install(&new, &releases, &binaries, &doctor, true)?;
    ensure!(
        !second.status.success(),
        "doctor must reject the new release"
    );
    ensure!(fs::read_link(releases.join("current"))? == previous);
    ensure!(fs::read_link(releases.join("last-good"))? == previous);
    ensure!(
        fs::read_to_string(binaries.join("oqto"))?.contains("test-old"),
        "managed entrypoint no longer resolves to the prior release"
    );
    Ok(())
}

#[test]
fn failed_fresh_entrypoint_creation_does_not_leave_current() -> Result<()> {
    let root = tempfile::tempdir()?;
    let releases = root.path().join("releases");
    let binaries = root.path().join("bin");
    fs::create_dir_all(&binaries)?;
    fs::set_permissions(&binaries, fs::Permissions::from_mode(0o500))?;
    let artifact = bundle(root.path(), "test-unwritable-bin")?;
    let doctor = rejecting_doctor(root.path())?;

    let output = install(&artifact, &releases, &binaries, &doctor, false)?;
    fs::set_permissions(&binaries, fs::Permissions::from_mode(0o700))?;
    ensure!(
        !output.status.success(),
        "install with unwritable bin directory unexpectedly passed"
    );
    ensure!(
        fs::symlink_metadata(releases.join("current")).is_err(),
        "failed link creation left current on an uninstalled release"
    );
    Ok(())
}

#[test]
fn reinstall_never_deletes_the_current_release_tree() -> Result<()> {
    let root = tempfile::tempdir()?;
    let releases = root.path().join("releases");
    let binaries = root.path().join("bin");
    let artifact = bundle(root.path(), "test-same-id")?;
    let doctor = rejecting_doctor(root.path())?;

    let first = install(&artifact, &releases, &binaries, &doctor, false)?;
    ensure!(first.status.success());
    let active = fs::read_link(releases.join("current"))?;
    let marker = active.join("prior-install-marker");
    fs::write(&marker, b"do not discard an active release")?;

    let second = install(&artifact, &releases, &binaries, &doctor, false)?;
    ensure!(
        !second.status.success(),
        "reinstall without a content-aware idempotence contract must fail closed"
    );
    ensure!(fs::read(&marker)? == b"do not discard an active release");
    ensure!(fs::read_link(releases.join("current"))? == active);
    Ok(())
}
