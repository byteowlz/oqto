//! Cross-platform fail-before-mutation contract for the public installer CLI.
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};

fn rejects_unsupported_artifact(root: &Path, suffix: &str) -> Result<()> {
    let artifact = root.join(format!("oqto-v0.5.0-{suffix}.tar.gz"));
    fs::write(&artifact, b"untrusted archive bytes")?;
    let checksum = root.join(format!("oqto-v0.5.0-{suffix}.tar.gz.sha256"));
    fs::write(
        &checksum,
        format!(
            "{:x}  {}\n",
            Sha256::digest(fs::read(&artifact)?),
            artifact.display()
        ),
    )?;
    let releases = root.join("releases");
    let entrypoints = root.join("entrypoints");
    let output = Command::new(env!("CARGO_BIN_EXE_oqto-setup"))
        .arg("install")
        .arg("--artifact")
        .arg(&artifact)
        .arg("--checksum")
        .arg(&checksum)
        .arg("--releases-root")
        .arg(&releases)
        .arg("--bin-dir")
        .arg(&entrypoints)
        .args(["--doctor-strict", "false"])
        .output()?;
    assert!(
        !output.status.success(),
        "unsupported {suffix} release activated"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported release target"), "{stderr}");
    assert!(
        !releases.exists(),
        "unsupported target created release root"
    );
    assert!(
        !entrypoints.exists(),
        "unsupported target created entrypoints"
    );
    Ok(())
}

#[test]
fn wrong_linux_cpu_fails_before_mutating_host() -> Result<()> {
    if std::env::consts::OS != "linux" {
        return Ok(());
    }
    let foreign = if std::env::consts::ARCH == "x86_64" {
        "aarch64"
    } else {
        "x86_64"
    };
    let root = tempfile::tempdir()?;
    rejects_unsupported_artifact(root.path(), &format!("{foreign}-unknown-linux-gnu"))
}

#[cfg(target_os = "macos")]
#[test]
fn mac_rejects_both_linux_and_unimplemented_native_release() -> Result<()> {
    let root = tempfile::tempdir()?;
    rejects_unsupported_artifact(root.path(), "aarch64-unknown-linux-gnu")?;
    rejects_unsupported_artifact(root.path(), "aarch64-apple-darwin")?;
    Ok(())
}
