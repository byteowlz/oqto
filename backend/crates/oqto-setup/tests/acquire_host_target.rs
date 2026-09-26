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
