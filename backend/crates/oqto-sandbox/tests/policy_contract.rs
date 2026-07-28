//! The frozen readability contract of the shipped profiles.
//!
//! This began as a differential test between the policy and the bwrap builder
//! it replaces. Once the builder is policy-driven that comparison is
//! self-referential, so the expectations below are recorded independently of
//! both: they are what the profiles enforced before the swap, asserted against
//! the resolver *and* against a real sandbox.
//!
//! A change here is a change in what an agent can see. Update the table only
//! with a reason.
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
fn sandbox_can_read(args: &[String], file: &Path) -> bool {
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

/// (profile, path, readable) as enforced before the policy swap.
///
/// `~` denotes the account home. Absent files are skipped: an absent path is
/// unreadable for reasons unrelated to policy.
const READABILITY_CONTRACT: &[(&str, &str, bool)] = &[
    // Credentials and platform secrets are denied by every profile.
    ("minimal", "~/.ssh/known_hosts", false),
    ("development", "~/.ssh/known_hosts", false),
    ("strict", "~/.ssh/known_hosts", false),
    (
        "minimal",
        "~/.local/share/oqto/credentials/jwt_secret",
        false,
    ),
    (
        "development",
        "~/.local/share/oqto/credentials/jwt_secret",
        false,
    ),
    (
        "strict",
        "~/.local/share/oqto/credentials/jwt_secret",
        false,
    ),
    ("minimal", "~/.config/oqto/config.toml", false),
    ("development", "~/.config/oqto/config.toml", false),
    ("strict", "~/.config/oqto/config.toml", false),
    ("minimal", "/etc/oqto/config.toml", false),
    ("development", "/etc/oqto/config.toml", false),
    ("strict", "/etc/oqto/config.toml", false),
    // Toolchain configuration stays readable, including under strict, where it
    // is granted explicitly rather than inherited from a readable home.
    ("minimal", "~/.cargo/env", true),
    ("development", "~/.cargo/env", true),
    ("strict", "~/.cargo/env", true),
    // strict reaches this through a denied parent; the effective policy of the
    // sandbox must remain visible to the agent running under it.
    ("minimal", "~/.config/oqto/sandbox.toml", true),
    ("development", "~/.config/oqto/sandbox.toml", true),
    ("strict", "~/.config/oqto/sandbox.toml", true),
    // System configuration is bound read-only for every profile.
    ("minimal", "/etc/hostname", true),
    ("development", "/etc/hostname", true),
    ("strict", "/etc/hostname", true),
];

fn probe_path(home: &Path, spec: &str) -> std::path::PathBuf {
    match spec.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => Path::new(spec).to_path_buf(),
    }
}

#[test]
fn the_resolver_reproduces_the_readability_contract() {
    let Some(home) = dirs::home_dir() else {
        eprintln!("skipping: no home directory");
        return;
    };
    let workspace = tempfile::tempdir_in(&home).expect("tempdir");
    let registry = ResourceRegistry::default();

    for (name, spec, expected) in READABILITY_CONTRACT {
        let profile = SandboxProfile::builtin(name).expect("builtin profile");
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
        let path = probe_path(&home, spec);
        let access = resolved.resolve(&path).expect("path resolves").access;
        assert_eq!(
            access >= Access::Read,
            *expected,
            "{name}: {spec} resolved to {access:?}, contract says readable={expected}"
        );
    }
}

#[test]
fn a_real_sandbox_enforces_the_readability_contract() {
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

    let mut compared = 0usize;
    for (name, spec, expected) in READABILITY_CONTRACT {
        let path = probe_path(&home, spec);
        if !path.is_file() {
            continue;
        }
        let config = SandboxConfig::from_profile(name);
        let Some(args) = config.build_bwrap_args_for_user(workspace.path(), None) else {
            panic!("{name}: builder produced no arguments");
        };
        assert_eq!(
            sandbox_can_read(&args, &path),
            *expected,
            "{name}: real sandbox disagrees with the contract for {spec}"
        );
        compared += 1;
    }
    assert!(
        compared >= 12,
        "too few contract paths exist on this host to be meaningful: {compared}"
    );
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
        !sandbox_can_read(&empty, &probe),
        "probe reported host access rather than sandbox access"
    );
}
