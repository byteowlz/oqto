//! A Linux-targeted dependency plan must not become local Mac or foreign-CPU
//! binaries. Staging a remote target without --install-bin remains separate.
use anyhow::Result;
use std::{fs, process::Command};

#[test]
fn foreign_tool_install_refuses_before_network_or_host_mutation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let manifest = root.path().join("dependencies.toml");
    fs::write(
        &manifest,
        "[oqto]\nversion = \"0.5.0\"\n[byteowlz]\nmmry = \"0.13.4\"\n",
    )?;
    let staged = root.path().join("not-created-staging");
    let binaries = root.path().join("bin");
    fs::create_dir(&binaries)?;
    fs::write(binaries.join("existing"), b"preserved tool")?;
    let foreign = if std::env::consts::OS == "linux" && std::env::consts::ARCH == "x86_64" {
        "aarch64"
    } else {
        "x86-64"
    };
    let output = Command::new(env!("CARGO_BIN_EXE_oqto-setup"))
        .arg("acquire")
        .arg("--manifest")
        .arg(&manifest)
        .args([
            "--arch",
            foreign,
            "--base-url",
            "file:///never-download-oqto-artifact",
        ])
        .arg("--dest")
        .arg(&staged)
        .arg("--install-bin")
        .arg(&binaries)
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported release target"), "{stderr}");
    assert!(!staged.exists(), "foreign target started acquisition");
    assert_eq!(fs::read(binaries.join("existing"))?, b"preserved tool");
    assert_eq!(fs::read_dir(&binaries)?.count(), 1);
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn incomplete_binary_owners_fail_before_download_or_mutation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let manifest = root.path().join("dependencies.toml");
    fs::write(
        &manifest,
        "[oqto]\nversion = '0.5.0'\n[byteowlz]\nmmry = '0.13.4'\n",
    )?;
    let staged = root.path().join("not-created-staging");
    let bin = root.path().join("bin");
    let host_arch = if std::env::consts::ARCH == "x86_64" {
        "x86-64"
    } else {
        "aarch64"
    };
    let output = Command::new(env!("CARGO_BIN_EXE_oqto-setup"))
        .args(["acquire", "--manifest"])
        .arg(&manifest)
        .args([
            "--arch",
            host_arch,
            "--base-url",
            "file:///no-such-tool-releases",
        ])
        .arg("--install-bin")
        .arg(&bin)
        .arg("--tools-only")
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("binary_owners"), "{stderr}");
    assert!(!staged.exists());
    assert!(!bin.exists());
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn local_install_rejects_caller_supplied_stage_before_download() -> Result<()> {
    let root = tempfile::tempdir()?;
    let manifest = root.path().join("dependencies.toml");
    fs::write(
        &manifest,
        "[oqto]\nversion = '0.5.0'\n[byteowlz]\nmmry = '0.13.4'\n[binary_owners]\nmmry = ['mmry']\n",
    )?;
    let stage = root.path().join("user-writable-stage");
    fs::create_dir(&stage)?;
    let outside = root.path().join("outside-user-file");
    fs::write(&outside, b"preserve user content")?;
    let arch = if std::env::consts::ARCH == "x86_64" {
        "x86-64"
    } else {
        "aarch64"
    };
    let triple = if std::env::consts::ARCH == "x86_64" {
        "x86_64-unknown-linux-gnu"
    } else {
        "aarch64-unknown-linux-gnu"
    };
    let archive_name = format!("mmry-v0.13.4-{triple}.tar.gz");
    std::os::unix::fs::symlink(&outside, stage.join(&archive_name))?;
    let bin = root.path().join("not-created-bin");
    let output = Command::new(env!("CARGO_BIN_EXE_oqto-setup"))
        .args(["acquire", "--manifest"])
        .arg(&manifest)
        .args(["--arch", arch, "--base-url", "file:///never-download"])
        .arg("--dest")
        .arg(&stage)
        .arg("--install-bin")
        .arg(&bin)
        .arg("--tools-only")
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--dest cannot be used with --install-bin"),
        "{stderr}"
    );
    assert_eq!(fs::read(outside)?, b"preserve user content");
    assert!(!bin.exists());
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn native_checked_tool_fixture_installs_without_clobbering_unmanaged_bin() -> Result<()> {
    use sha2::{Digest, Sha256};
    let root = tempfile::tempdir()?;
    let manifest = root.path().join("dependencies.toml");
    fs::write(
        &manifest,
        "[oqto]\nversion = '0.5.0'\n[byteowlz]\nmmry = '0.13.4'\n[binary_owners]\nmmry = ['mmry']\n",
    )?;
    let arch = if std::env::consts::ARCH == "x86_64" {
        "x86-64"
    } else {
        "aarch64"
    };
    let triple = if std::env::consts::ARCH == "x86_64" {
        "x86_64-unknown-linux-gnu"
    } else {
        "aarch64-unknown-linux-gnu"
    };
    let name = format!("mmry-v0.13.4-{triple}.tar.gz");
    let release = root.path().join("source/mmry/releases/download/v0.13.4");
    fs::create_dir_all(&release)?;
    let archive = release.join(&name);
    let encoded =
        flate2::write::GzEncoder::new(fs::File::create(&archive)?, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoded);
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(4);
    header.set_mode(0o755);
    header.set_cksum();
    builder.append_data(
        &mut header,
        format!("mmry-v0.13.4-{triple}/bin/mmry"),
        &b"demo"[..],
    )?;
    builder.into_inner()?.finish()?;
    fs::write(
        release.join("checksums.txt"),
        format!("{:x}  {name}\n", Sha256::digest(fs::read(&archive)?)),
    )?;
    let bin = root.path().join("bin");
    fs::create_dir(&bin)?;
    fs::write(bin.join("unmanaged"), b"preserved")?;
    let output = Command::new(env!("CARGO_BIN_EXE_oqto-setup"))
        .args(["acquire", "--manifest"])
        .arg(&manifest)
        .args(["--arch", arch, "--base-url"])
        .arg(format!("file://{}", root.path().join("source").display()))
        .arg("--install-bin")
        .arg(&bin)
        .arg("--tools-only")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(bin.join("mmry"))?, b"demo");
    assert_eq!(fs::read(bin.join("unmanaged"))?, b"preserved");
    assert!(!root.path().join("staging").exists());
    assert!(!bin.join(".mmry.new").exists());
    Ok(())
}
