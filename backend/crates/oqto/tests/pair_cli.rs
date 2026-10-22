//! Public pairing dispatch must not initialize the server or mutate a profile.
use anyhow::{Result, ensure};
use std::process::{Command, Stdio};

#[test]
fn pair_help_exposes_shared_flags() -> Result<()> {
    let result = Command::new(env!("CARGO_BIN_EXE_oqto"))
        .args(["pair", "--help"])
        .output()?;
    ensure!(result.status.success());
    let text = String::from_utf8(result.stdout)?;
    for flag in [
        "--workspace",
        "--network",
        "--interactive",
        "--non-interactive",
        "--yes",
        "--status",
    ] {
        ensure!(text.contains(flag));
    }
    Ok(())
}

#[test]
fn public_headless_validation_and_server_options_never_initialize_state() -> Result<()> {
    for args in [
        vec!["pair", "--output", "json"],
        vec!["--json", "pair"],
        vec!["--yes", "pair", "--interactive"],
        vec!["pair", "--interactive", "--yes"],
        vec![
            "--config",
            "/missing-pair-server-config",
            "pair",
            "--status",
        ],
        vec!["pair", "--dry-run", "--status"],
        vec!["--json", "pair", "--dry-run", "--status"],
    ] {
        let home = tempfile::tempdir()?;
        let result = Command::new(env!("CARGO_BIN_EXE_oqto"))
            .args(&args)
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join(".config"))
            .current_dir(home.path())
            .stdin(Stdio::null())
            .output()?;
        ensure!(!result.status.success(), "unexpected admission: {args:?}");
        ensure!(!home.path().join(".local").exists() && !home.path().join(".config").exists());
        if args == ["pair", "--output", "json"] || args.first() == Some(&"--json") {
            ensure!(result.status.code() == Some(2));
            let error: serde_json::Value = serde_json::from_slice(&result.stdout)?;
            ensure!(error["ok"] == false && error["error"]["code"] == "usage");
        }
    }
    Ok(())
}
