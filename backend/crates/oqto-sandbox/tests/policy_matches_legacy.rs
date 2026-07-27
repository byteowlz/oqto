//! Differential proof that ADR-0028 translation preserves current enforcement.
//!
//! The translation is only trustworthy if the policy predicts what the existing
//! bwrap builder actually enforces. This runs the *legacy* argument builder
//! under real bwrap and compares observed readability against the policy's
//! prediction for the same path and profile.
//!
//! Probes are read-only by construction: they run against real paths in the
//! developer's home, so nothing here may create, append to, or remove a file.
//! Write behaviour is covered by the fixture-based `policy_equivalence` suite,
//! which owns the tree it mutates.

use oqto_sandbox::{
    SandboxConfig, SandboxProfile,
    path_policy::{Access, ResolutionContext, ResourceRegistry},
    policy_translate::translate_profile,
};
use std::{path::Path, process::Command};

fn bwrap_available() -> bool {
    if !SandboxConfig::is_bwrap_available() {
        return false;
    }
    matches!(
        Command::new("bwrap").args(["--dev-bind", "/", "/", "true"]).status(),
        Ok(status) if status.success()
    )
}

/// Can the sandbox read this file's contents?
fn legacy_can_read(args: &[String], file: &Path) -> bool {
    let output = Command::new("bwrap")
        .args(args)
        .arg("/usr/bin/sh")
        .arg("-c")
        .arg(format!(
            "cat {} >/dev/null 2>&1 && echo READ",
            file.display()
        ))
        .output()
        .expect("spawning bwrap");
    String::from_utf8_lossy(&output.stdout).contains("READ")
}

#[test]
fn every_builtin_profile_enforces_what_the_policy_predicts() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap unavailable");
        return;
    }
    let Some(home) = dirs::home_dir() else {
        eprintln!("skipping: no home directory");
        return;
    };
    // Not under /tmp: the profiles give /tmp a private tmpfs after binding the
    // workspace, so a workspace there is masked and bwrap cannot even chdir
    // into it. Nothing would run and every probe would read as denied.
    let workspace = tempfile::tempdir_in(&home).expect("tempdir");

    // Existing files only: an absent path is unreadable for reasons unrelated
    // to policy and would make the comparison vacuous.
    let probes: Vec<_> = [
        home.join(".ssh/known_hosts"),
        home.join(".cargo/env"),
        home.join(".config/oqto/config.toml"),
        home.join(".config/oqto/sandbox.toml"),
        home.join(".local/share/oqto/credentials/jwt_secret"),
        Path::new("/etc/oqto/config.toml").to_path_buf(),
        Path::new("/etc/hostname").to_path_buf(),
    ]
    .into_iter()
    .filter(|path| path.is_file())
    .collect();
    assert!(
        probes.len() >= 4,
        "too few probe files exist to make this meaningful: {probes:?}"
    );

    let registry = ResourceRegistry::default();
    let mut compared = 0usize;

    for name in ["minimal", "development", "strict"] {
        let profile = SandboxProfile::builtin(name).expect("builtin profile");
        let config = SandboxConfig::from_profile(name);
        let Some(legacy_args) = config.build_bwrap_args_for_user(workspace.path(), None) else {
            panic!("{name}: legacy builder produced no arguments");
        };

        let resolved = translate_profile(name, &profile)
            .expect("profile translates")
            .policy
            .resolve_roots(&ResolutionContext {
                workdir: workspace.path(),
                home: &home,
                resources: &registry,
            })
            .expect("roots resolve")
            .policy;

        for probe in &probes {
            let predicted = resolved.resolve(probe).expect("path resolves").access;
            let predicted_readable = predicted >= Access::Read;
            let observed_readable = legacy_can_read(&legacy_args, probe);
            assert_eq!(
                predicted_readable,
                observed_readable,
                "{name}: policy predicts readable={predicted_readable} ({predicted:?}) but the \
                 legacy sandbox observed readable={observed_readable} for {}",
                probe.display()
            );
            compared += 1;
        }
    }

    assert!(compared >= 12, "expected a meaningful comparison count");
}

#[test]
fn the_comparison_can_fail() {
    // Negative control: if the probe reported the host's view instead of the
    // sandbox's, every path would read as accessible and the test above would
    // pass vacuously. A sandbox that binds nothing must see nothing.
    if !bwrap_available() {
        eprintln!("skipping: bwrap unavailable");
        return;
    }
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let probe = home.join(".ssh/known_hosts");
    if !probe.is_file() {
        return;
    }
    let empty = vec![
        "--ro-bind".to_string(),
        "/usr".to_string(),
        "/usr".to_string(),
        "--symlink".to_string(),
        "usr/bin".to_string(),
        "/bin".to_string(),
        "--symlink".to_string(),
        "usr/lib".to_string(),
        "/lib".to_string(),
        "--symlink".to_string(),
        "usr/lib".to_string(),
        "/lib64".to_string(),
    ];
    assert!(
        !legacy_can_read(&empty, &probe),
        "probe reported host access rather than sandbox access"
    );
}
