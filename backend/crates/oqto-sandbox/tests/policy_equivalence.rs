//! Prove the ADR-0028 adapter enforces what the policy predicts.
//!
//! The policy resolver is pure, so it can only be trusted as a description of
//! enforcement if a real sandbox agrees with it. These tests compile a resolved
//! policy to bwrap arguments, run bwrap, and compare observed access against
//! the resolver's answer for the same path.

use oqto_sandbox::{
    SandboxConfig,
    path_policy::{Access, PolicyLayer, ResolvedPolicy, ResolvedRule, RuleOrigin},
    policy_bwrap::{HostPaths, compile_filesystem_args},
};
use std::{fs, path::Path, process::Command};

fn bwrap_available() -> bool {
    if !SandboxConfig::is_bwrap_available() {
        return false;
    }
    matches!(
        Command::new("bwrap").args(["--dev-bind", "/", "/", "true"]).status(),
        Ok(status) if status.success()
    )
}

fn origin() -> RuleOrigin {
    RuleOrigin {
        layer: PolicyLayer::Admin,
        source: "equivalence-test".to_string(),
    }
}

/// What a probe observed inside the sandbox, expressed as policy access.
fn observed_access(args: &[String], file: &Path) -> Access {
    let path = file.display();
    // The write probe is nested inside the read probe on purpose. A masked path
    // is an empty tmpfs, which is writable: appending there creates a new file
    // and would report write access to content that is actually hidden. Access
    // is only meaningful against the real path, so reachability is established
    // first.
    let script = format!(
        "if cat {path} >/dev/null 2>&1; then \
           echo READ; \
           if echo probe >> {path} 2>/dev/null; then echo WRITE; fi; \
         fi"
    );
    let output = Command::new("bwrap")
        .args(args)
        .args(["--ro-bind", "/usr", "/usr"])
        // /bin and /lib are symlinks into /usr on merged-usr systems, so they
        // are recreated as symlinks rather than bound.
        .args(["--symlink", "usr/bin", "/bin"])
        .args(["--symlink", "usr/lib", "/lib"])
        .args(["--symlink", "usr/lib", "/lib64"])
        .args(["--proc", "/proc"])
        .args(["--dev", "/dev"])
        .arg("/usr/bin/sh")
        .arg("-c")
        .arg(script)
        .output()
        .expect("spawning bwrap");
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.contains("WRITE") {
        Access::Write
    } else if stdout.contains("READ") {
        Access::Read
    } else {
        Access::None
    }
}

/// Build a tree whose paths exercise grant, mask, and refinement together.
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("tempdir");
    for relative in ["open", "readonly", "secret", "secret/nested"] {
        fs::create_dir_all(root.path().join(relative)).expect("mkdir");
    }
    for relative in [
        "open/file",
        "readonly/file",
        "secret/file",
        "secret/nested/file",
    ] {
        fs::write(root.path().join(relative), "contents\n").expect("write");
    }
    root
}

#[test]
fn real_bwrap_agrees_with_the_resolver() {
    if !bwrap_available() {
        eprintln!("skipping: bwrap unavailable");
        return;
    }
    let fixture = fixture();
    let base = fixture.path();

    let mut policy = ResolvedPolicy::new(Access::None, origin());
    for (relative, access) in [
        ("open", Access::Write),
        ("readonly", Access::Read),
        ("secret", Access::None),
        // A grant deeper than a mask must survive it.
        ("secret/nested", Access::Read),
    ] {
        policy.add_rule(
            ResolvedRule::new(base.join(relative), access, origin()).expect("valid rule"),
        );
    }

    let args = compile_filesystem_args(&policy, &HostPaths);

    for (relative, expected) in [
        ("open/file", Access::Write),
        ("readonly/file", Access::Read),
        ("secret/file", Access::None),
        ("secret/nested/file", Access::Read),
    ] {
        let file = base.join(relative);
        let predicted = policy.resolve(&file).expect("resolves").access;
        assert_eq!(
            predicted, expected,
            "resolver disagrees with the test's own expectation for {relative}"
        );
        assert_eq!(
            observed_access(&args, &file),
            expected,
            "bwrap enforcement differs from the resolved policy for {relative}"
        );
    }
}

#[test]
fn a_masked_path_is_not_merely_unbound() {
    // Negative control: if the mask were dropped the probe would still fail,
    // because nothing bound the path. Binding the parent writable first makes
    // the mask the only thing that can deny the child.
    if !bwrap_available() {
        eprintln!("skipping: bwrap unavailable");
        return;
    }
    let fixture = fixture();
    let base = fixture.path();

    let mut without_mask = ResolvedPolicy::new(Access::None, origin());
    without_mask.add_rule(ResolvedRule::new(base, Access::Write, origin()).expect("valid rule"));
    assert_eq!(
        observed_access(
            &compile_filesystem_args(&without_mask, &HostPaths),
            &base.join("secret/file")
        ),
        Access::Write,
        "control failed: the parent grant should expose the child"
    );

    let mut with_mask = without_mask.clone();
    with_mask.add_rule(
        ResolvedRule::new(base.join("secret"), Access::None, origin()).expect("valid rule"),
    );
    assert_eq!(
        observed_access(
            &compile_filesystem_args(&with_mask, &HostPaths),
            &base.join("secret/file")
        ),
        Access::None,
        "the mask did not deny a path its parent grant exposed"
    );
}
