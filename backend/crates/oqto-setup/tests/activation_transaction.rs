//! Transactional release activation through the real oqto-setup CLI.
//! Linux-only until the macOS release has its own launchd/installer contract.
//! All installation paths are redirected into a temporary directory.
#![cfg(target_os = "linux")]

use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
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
        let body = if name == &"oqtoctl" {
            format!("#!/bin/sh\n[ \"$1\" != doctor ] || exit 1\nprintf '%s\\n' '{id}'\n")
        } else {
            format!("#!/bin/sh\nprintf '%s\\n' '{id}'\n")
        };
        fs::write(&binary, body)?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))?;
    }
    if complete {
        fs::write(
            staging.join("manifest.toml"),
            "manifest_version = 1\nid = \"oqto-dist\"\n[release]\ntarget = \"full\"\n",
        )?;
        fs::create_dir_all(staging.join("immutable/frontend/dist"))?;
        fs::write(
            staging.join("immutable/frontend/dist/index.html"),
            b"<!doctype html><main>Oqto</main>",
        )?;
    }
    let artifact = root.join(format!("{name}.tar.gz"));
    // macOS bsdtar otherwise inserts AppleDouble ._ members (provenance
    // xattrs) outside the declared release root, which must stay forbidden.
    let status = Command::new("tar")
        .env("COPYFILE_DISABLE", "1")
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
    // Fixtures generate their own checksums; production callers must obtain
    // theirs from an independent trusted release source.
    let checksum = artifact.with_file_name(format!(
        "{}.sha256",
        artifact.file_name().unwrap().to_string_lossy()
    ));
    let digest = Sha256::digest(fs::read(artifact)?);
    fs::write(&checksum, format!("{digest:x}  {}\n", artifact.display()))?;
    let output = Command::new(env!("CARGO_BIN_EXE_oqto-setup"))
        .arg("install")
        .arg("--artifact")
        .arg(artifact)
        .arg("--checksum")
        .arg(&checksum)
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

fn crafted_bundle(root: &Path, id: &str, variant: &str) -> Result<std::path::PathBuf> {
    let name = format!("oqto-{id}-x86_64-unknown-linux-gnu");
    let artifact = root.join(format!("{name}.tar.gz"));
    let script = r#"
import io,sys,tarfile
artifact,root,variant = sys.argv[1:]
def file(tar,name,data,mode=0o644):
    item=tarfile.TarInfo(name); item.size=len(data); item.mode=mode
    tar.addfile(item,io.BytesIO(data))
with tarfile.open(artifact,'w:gz') as tar:
    file(tar,root+'/manifest.toml',b'manifest_version = 1\nid = "oqto-dist"\n[release]\ntarget = "full"\n')
    for name in ['oqto','oqtoctl','oqto-setup','oqto-runner','oqto-files','oqto-sandbox','oqto-usermgr','pi-bridge']:
        file(tar,root+'/immutable/bin/'+name,b'#!/bin/true\n',0o755)
    if variant=='traversal': file(tar,root+'/../../outside',b'outside')
    if variant=='second-root': file(tar,'unrelated/other',b'other')
    if variant=='duplicate': file(tar,root+'/manifest.toml',b'manifest_version = 1\nid = "oqto-dist"\n[release]\ntarget = "full"\n')
    if variant=='symlink':
        entry=tarfile.TarInfo(root+'/escape'); entry.type=tarfile.SYMTYPE; entry.linkname='../outside'
        tar.addfile(entry); file(tar,root+'/escape/payload',b'outside')
    if variant=='hardlink':
        entry=tarfile.TarInfo(root+'/copy'); entry.type=tarfile.LNKTYPE; entry.linkname='../outside'
        tar.addfile(entry)
"#;
    let output = Command::new("python3")
        .args(["-c", script])
        .arg(&artifact)
        .args([&name, variant])
        .output()?;
    ensure!(output.status.success(), "failed to build crafted archive");
    Ok(artifact)
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
fn full_release_without_frontend_fails_before_deferred_activation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let artifact = bundle(root.path(), "no-frontend")?;
    let archive_root = "oqto-no-frontend-x86_64-unknown-linux-gnu";
    fs::remove_file(
        root.path()
            .join("stage")
            .join(archive_root)
            .join("immutable/frontend/dist/index.html"),
    )?;
    let status = Command::new("tar")
        .env("COPYFILE_DISABLE", "1")
        .arg("-C")
        .arg(root.path().join("stage"))
        .arg("-czf")
        .arg(&artifact)
        .arg(archive_root)
        .status()?;
    ensure!(
        status.success(),
        "failed creating frontend-negative test artifact"
    );
    let releases = root.path().join("releases");
    let bins = root.path().join("bin");
    let output = install(
        &artifact,
        &releases,
        &bins,
        &rejecting_doctor(root.path())?,
        false,
    )?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("frontend"), "{stderr}");
    assert!(!releases.join("current").exists());
    assert!(!bins.join("oqto").exists());
    Ok(())
}

#[test]
fn install_without_checksum_fails_before_release_mutation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let artifact = bundle(root.path(), "missing-checksum")?;
    let releases = root.path().join("releases");
    let bins = root.path().join("bin");
    let output = Command::new(env!("CARGO_BIN_EXE_oqto-setup"))
        .arg("install")
        .arg("--artifact")
        .arg(&artifact)
        .arg("--releases-root")
        .arg(&releases)
        .arg("--bin-dir")
        .arg(&bins)
        .args(["--doctor-strict", "false"])
        .output()?;
    ensure!(
        !output.status.success(),
        "missing checksum activated an unverified artifact"
    );
    ensure!(fs::symlink_metadata(releases.join("current")).is_err());
    ensure!(fs::symlink_metadata(bins.join("oqto")).is_err());
    Ok(())
}

#[test]
fn mismatched_checksum_fails_before_release_mutation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let artifact = bundle(root.path(), "bad-checksum")?;
    let checksum = root.path().join("wrong.sha256");
    fs::write(
        &checksum,
        format!("{}  {}\n", "0".repeat(64), artifact.display()),
    )?;
    let releases = root.path().join("releases");
    let bins = root.path().join("bin");
    let output = Command::new(env!("CARGO_BIN_EXE_oqto-setup"))
        .arg("install")
        .arg("--artifact")
        .arg(&artifact)
        .arg("--checksum")
        .arg(&checksum)
        .arg("--releases-root")
        .arg(&releases)
        .arg("--bin-dir")
        .arg(&bins)
        .args(["--doctor-strict", "false"])
        .output()?;
    ensure!(
        !output.status.success(),
        "mismatched checksum activated an artifact"
    );
    ensure!(!releases.exists(), "checksum mismatch created release root");
    ensure!(fs::symlink_metadata(bins.join("oqto")).is_err());
    Ok(())
}

#[test]
fn checksum_for_different_filename_cannot_verify_a_release() -> Result<()> {
    let root = tempfile::tempdir()?;
    let artifact = bundle(root.path(), "wrong-checksum-name")?;
    let checksum = root.path().join("ambiguous.sha256");
    let digest = Sha256::digest(fs::read(&artifact)?);
    fs::write(&checksum, format!("{digest:x}  another-release.tar.gz\n"))?;
    let releases = root.path().join("releases");
    let bins = root.path().join("bin");
    let output = Command::new(env!("CARGO_BIN_EXE_oqto-setup"))
        .arg("install")
        .arg("--artifact")
        .arg(&artifact)
        .arg("--checksum")
        .arg(&checksum)
        .arg("--releases-root")
        .arg(&releases)
        .arg("--bin-dir")
        .arg(&bins)
        .args(["--doctor-strict", "false"])
        .output()?;
    ensure!(
        !output.status.success(),
        "wrong-named checksum verified an artifact"
    );
    ensure!(!releases.exists());
    Ok(())
}

#[test]
fn strict_doctor_does_not_use_an_unrelated_host_binary() -> Result<()> {
    let root = tempfile::tempdir()?;
    let releases = root.path().join("releases");
    let bins = root.path().join("bin");
    let id = "strict-wrong-binary";
    let artifact = bundle(root.path(), id)?;
    let name = format!("oqto-{id}-x86_64-unknown-linux-gnu");
    let staged = root.path().join("stage").join(&name);
    let ctl = staged.join("immutable/bin/oqtoctl");
    // Still executable in the archive, but cannot be spawned (missing shebang).
    fs::write(&ctl, "#!/definitely/missing/interpreter\n")?;
    fs::set_permissions(&ctl, fs::Permissions::from_mode(0o755))?;
    ensure!(
        Command::new("tar")
            .arg("-C")
            .arg(root.path().join("stage"))
            .arg("-czf")
            .arg(&artifact)
            .arg(&name)
            .status()?
            .success()
    );
    let external = root.path().join("unrelated-doctor");
    fs::create_dir(&external)?;
    fs::write(external.join("oqtoctl"), "#!/bin/sh\nexit 0\n")?;
    fs::set_permissions(external.join("oqtoctl"), fs::Permissions::from_mode(0o755))?;
    let output = install(&artifact, &releases, &bins, &external, true)?;
    ensure!(
        !output.status.success(),
        "ambient oqtoctl falsely verified a broken staged binary"
    );
    ensure!(fs::symlink_metadata(releases.join("current")).is_err());
    ensure!(fs::symlink_metadata(bins.join("oqtoctl")).is_err());
    Ok(())
}

#[test]
fn strict_doctor_runs_the_staged_binary_even_when_path_rejects() -> Result<()> {
    let root = tempfile::tempdir()?;
    let releases = root.path().join("releases");
    let bins = root.path().join("bin");
    let id = "strict-staged";
    let artifact = bundle(root.path(), id)?;
    let name = format!("oqto-{id}-x86_64-unknown-linux-gnu");
    let staged = root.path().join("stage").join(&name);
    let ctl = staged.join("immutable/bin/oqtoctl");
    fs::write(&ctl, "#!/bin/sh\n[ \"$1\" = doctor ] || exit 1\nexit 0\n")?;
    fs::set_permissions(&ctl, fs::Permissions::from_mode(0o755))?;
    ensure!(
        Command::new("tar")
            .arg("-C")
            .arg(root.path().join("stage"))
            .arg("-czf")
            .arg(&artifact)
            .arg(&name)
            .status()?
            .success()
    );
    let output = install(
        &artifact,
        &releases,
        &bins,
        &rejecting_doctor(root.path())?,
        true,
    )?;
    ensure!(
        output.status.success(),
        "staged doctor was ignored: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    ensure!(fs::read_link(releases.join("current"))? == releases.join(&name));
    ensure!(fs::read_link(releases.join("last-good"))? == releases.join(&name));
    Ok(())
}

#[test]
fn corrupt_archive_does_not_leave_an_unreferenced_stage() -> Result<()> {
    let root = tempfile::tempdir()?;
    let releases = root.path().join("releases");
    let artifact = root
        .path()
        .join("oqto-test-corrupt-x86_64-unknown-linux-gnu.tar.gz");
    fs::write(&artifact, b"not a gzip tar archive")?;
    let output = install(
        &artifact,
        &releases,
        &root.path().join("bin"),
        &rejecting_doctor(root.path())?,
        false,
    )?;
    ensure!(!output.status.success());
    ensure!(
        !releases
            .join("oqto-test-corrupt-x86_64-unknown-linux-gnu")
            .exists(),
        "failed archive extraction left a staged release"
    );
    ensure!(fs::symlink_metadata(releases.join("current")).is_err());
    Ok(())
}

#[test]
fn truncated_gzip_and_oversized_entry_leave_no_stage() -> Result<()> {
    for variant in ["truncated", "oversized"] {
        let root = tempfile::tempdir()?;
        let id = format!("bad-{variant}");
        let name = format!("oqto-{id}-x86_64-unknown-linux-gnu");
        let artifact = root.path().join(format!("{name}.tar.gz"));
        if variant == "truncated" {
            bundle(root.path(), &id)?;
            let bytes = fs::read(&artifact)?;
            fs::write(&artifact, &bytes[..bytes.len() - 8])?;
        } else {
            let script = "import gzip,sys,tarfile\nname,path=sys.argv[1:]\n".to_string()
                + "item=tarfile.TarInfo(name+'/oversized'); item.size=2147483649\n"
                + "with gzip.open(path,'wb') as out: out.write(item.tobuf()+bytes(1024))\n";
            ensure!(
                Command::new("python3")
                    .args(["-c", &script, &name])
                    .arg(&artifact)
                    .status()?
                    .success()
            );
        }
        let releases = root.path().join("releases");
        let output = install(
            &artifact,
            &releases,
            &root.path().join("bin"),
            &rejecting_doctor(root.path())?,
            false,
        )?;
        ensure!(!output.status.success(), "{variant} archive was activated");
        ensure!(
            !releases.join(&name).exists(),
            "{variant} left a staged release"
        );
        ensure!(fs::symlink_metadata(releases.join("current")).is_err());
    }
    Ok(())
}

#[test]
fn unsafe_or_ambiguous_archive_never_activates_or_writes_outside_stage() -> Result<()> {
    for variant in [
        "traversal",
        "second-root",
        "duplicate",
        "symlink",
        "hardlink",
    ] {
        let root = tempfile::tempdir()?;
        let id = format!("unsafe-{variant}");
        let artifact = crafted_bundle(root.path(), &id, variant)?;
        let releases = root.path().join("releases");
        let binaries = root.path().join("bin");
        let output = install(
            &artifact,
            &releases,
            &binaries,
            &rejecting_doctor(root.path())?,
            false,
        )?;
        ensure!(!output.status.success(), "{variant} archive was activated");
        ensure!(
            !releases
                .join(format!("oqto-{id}-x86_64-unknown-linux-gnu"))
                .exists(),
            "{variant} archive left a stage"
        );
        ensure!(
            !root.path().join("outside").exists(),
            "{variant} escaped the stage"
        );
        ensure!(fs::symlink_metadata(releases.join("current")).is_err());
        ensure!(fs::symlink_metadata(binaries.join("oqto")).is_err());
    }
    Ok(())
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
