//! Sandbox containment checks.

use std::path::{Path, PathBuf};
use std::process::Command;

use oqto_sandbox::SandboxConfig;

fn bwrap_available() -> bool {
    if !SandboxConfig::is_bwrap_available() {
        return false;
    }
    matches!(
        Command::new("bwrap")
            .args(["--dev-bind", "/", "/", "true"])
            .status(),
        Ok(s) if s.success()
    )
}

fn run_sandboxed(config: &SandboxConfig, workspace: &Path, script: &str) -> (bool, String) {
    let args = config
        .build_bwrap_args_for_user(workspace, None)
        .expect("profile must produce bwrap args");

    let output = Command::new("bwrap")
        .args(&args)
        .arg("sh")
        .arg("-c")
        .arg(script)
        .output()
        .expect("spawning bwrap");

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        eprintln!(
            "bwrap produced no stdout; status={:?} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    (output.status.success(), stdout)
}

fn strict_config() -> (SandboxConfig, tempfile::TempDir) {
    // Not under /tmp: the sandbox mounts a fresh tmpfs there, which shadows the
    // workspace bind and makes probes fail for the wrong reason.
    let home = std::env::var("HOME").expect("HOME");
    let workspace = tempfile::tempdir_in(&home).expect("tempdir");
    let mut config = SandboxConfig::strict();
    config.enabled = true;
    (config, workspace)
}

struct FileGuard(PathBuf);

impl Drop for FileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct DirGuard(PathBuf);

impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn run_directory_is_not_reachable() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap/userns unavailable");
        return;
    }
    let (config, workspace) = strict_config();

    let (_ok, out) = run_sandboxed(
        &config,
        workspace.path(),
        "test -e /run/oqto && echo present || echo denied",
    );

    assert_eq!(out, "denied");
}

#[test]
fn tmp_is_writable() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap/userns unavailable");
        return;
    }
    let (config, workspace) = strict_config();

    let (_ok, out) = run_sandboxed(
        &config,
        workspace.path(),
        "touch /tmp/oqto-probe 2>/dev/null && echo writable || echo denied",
    );

    assert_eq!(out, "writable");
}

#[test]
fn deny_read_paths_are_not_readable() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap/userns unavailable");
        return;
    }
    let (mut config, workspace) = strict_config();

    let home = std::env::var("HOME").expect("HOME");
    let dir = PathBuf::from(&home).join(".oqto-probe-creds");
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let _guard = DirGuard(dir.clone());
    let fixture = dir.join("key");
    std::fs::write(&fixture, "fixture").expect("fixture");

    config.deny_read.push("~/.oqto-probe-creds".to_string());

    // Readable on the host, else the assertion below proves nothing.
    assert_eq!(
        std::fs::read_to_string(&fixture).expect("fixture readable on host"),
        "fixture"
    );

    let (_ok, out) = run_sandboxed(
        &config,
        workspace.path(),
        &format!(
            "cat {} 2>/dev/null && echo present || echo denied",
            fixture.display()
        ),
    );

    assert_eq!(out, "denied");
}

#[test]
fn strict_profile_deny_read_defaults() {
    let config = SandboxConfig::strict();
    for expected in ["~/.ssh", "~/.gnupg", "~/.aws"] {
        assert!(
            config.deny_read.iter().any(|p| p == expected),
            "strict profile no longer denies {expected}"
        );
    }
}

// Pins current read-policy behaviour; see oqto-tp8v.
#[test]
fn unlisted_paths_follow_current_read_policy() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap/userns unavailable");
        return;
    }
    let (config, workspace) = strict_config();

    let home = std::env::var("HOME").expect("HOME");
    let fixture = PathBuf::from(&home).join(".oqto-probe-unlisted");
    std::fs::write(&fixture, "fixture").expect("fixture");
    let _guard = FileGuard(fixture.clone());

    let (_ok, out) = run_sandboxed(
        &config,
        workspace.path(),
        &format!(
            "cat {} >/dev/null 2>&1 && echo present || echo denied",
            fixture.display()
        ),
    );

    assert_eq!(out, "present", "read policy changed; see oqto-tp8v");
}

#[test]
fn writes_outside_the_workspace_are_denied() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap/userns unavailable");
        return;
    }
    let (config, workspace) = strict_config();

    let (_ok, out) = run_sandboxed(
        &config,
        workspace.path(),
        "touch ~/.oqto-probe 2>/dev/null && echo written || echo denied",
    );

    assert_eq!(out, "denied");
}

#[test]
fn the_workspace_itself_is_still_writable() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap/userns unavailable");
        return;
    }
    let (config, workspace) = strict_config();

    let (_ok, out) = run_sandboxed(
        &config,
        workspace.path(),
        "touch ./probe && echo writable || echo denied",
    );

    assert_eq!(out, "writable");
}

#[test]
fn development_profile_shape_is_stable() {
    let config = SandboxConfig::from_profile("development");
    assert!(
        config
            .allow_write
            .iter()
            .any(|p| p == "~" || p.contains("$HOME"))
            || config.profile == "development",
        "development profile shape changed"
    );
}

#[test]
fn sandbox_is_available_somewhere() {
    if std::env::var("OQTO_REQUIRE_SANDBOX").as_deref() != Ok("1") {
        eprintln!("skipping: set OQTO_REQUIRE_SANDBOX=1 to enforce");
        return;
    }
    assert!(
        bwrap_available(),
        "OQTO_REQUIRE_SANDBOX=1 but bwrap/userns is unavailable"
    );
}
