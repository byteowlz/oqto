#![cfg(not(target_os = "linux"))]

use anyhow::Result;
use std::process::Command;

#[test]
fn seccomp_compiler_refuses_non_linux_without_creating_output() -> Result<()> {
    let root = tempfile::tempdir()?;
    let out = root.path().join("filters");
    let output = Command::new(env!("CARGO_BIN_EXE_seccomp-policy-gen"))
        .args(["--policy", "unused-policy.toml", "--out-dir"])
        .arg(&out)
        .output()?;
    assert!(
        !output.status.success(),
        "non-Linux seccomp compiler succeeded"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("requires Linux"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!out.exists());
    Ok(())
}

#[test]
fn egress_relay_refuses_non_linux_before_listening() -> Result<()> {
    let output = Command::new(env!("CARGO_BIN_EXE_oqto-egress-relay")).output()?;
    assert!(!output.status.success(), "non-Linux egress relay started");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("requires Linux"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
