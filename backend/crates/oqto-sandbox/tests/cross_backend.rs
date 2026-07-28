//! Two adapters must not disagree about the effective access of a path.
//!
//! Both compile the same resolved policy, so agreement is structural rather
//! than coincidental. What these tests guard is the failure that would break
//! that: an adapter silently dropping a rule it cannot express, which would
//! make one backend quietly more permissive than the other.
//!
//! Note the limit of this file. It proves the *emission* agrees. Seatbelt
//! enforcement is not verified here because that requires macOS; the bwrap side
//! is verified by execution in `policy_equivalence` and `containment`.

use oqto_sandbox::{
    SandboxProfile,
    capability::{
        BackendCapabilities, Guarantee, OnMissingCapability, RequiredGuarantees, authorise,
        check_policy,
    },
    path_policy::{Access, ResolutionContext, ResolvedPolicy, ResourceRegistry},
    policy_bwrap::{PathPresence, compile_filesystem_args},
    policy_seatbelt::compile_profile,
    policy_translate::translate_profile,
};
use std::path::{Path, PathBuf};

/// The Seatbelt adapter emits resolved paths, because SBPL matches the resolved
/// form. bwrap does not: it operates on mounts and follows symlinks itself.
/// The asymmetry is real, so assertions against each backend use its own form.
fn resolved_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

struct AllPresent;
impl PathPresence for AllPresent {
    fn exists(&self, _path: &Path) -> bool {
        true
    }
    fn is_file(&self, _path: &Path) -> bool {
        false
    }
}

fn resolved(profile_name: &str) -> ResolvedPolicy {
    let profile = SandboxProfile::builtin(profile_name).expect("builtin profile");
    let registry = ResourceRegistry::default();
    translate_profile(profile_name, &profile)
        .expect("translates")
        .policy
        .resolve_roots(&ResolutionContext {
            workdir: Path::new("/work/project"),
            home: Path::new("/home/agent"),
            resources: &registry,
        })
        .expect("roots resolve")
        .policy
}

#[test]
fn neither_adapter_drops_a_rule() {
    for name in ["minimal", "development", "strict"] {
        let policy = resolved(name);
        let bwrap = compile_filesystem_args(&policy, &AllPresent);
        let sbpl = compile_profile(&policy);

        for rule in policy.rules() {
            let path = rule.path().to_string_lossy().to_string();
            assert!(
                bwrap.iter().any(|arg| arg == &path),
                "{name}: bwrap dropped {path}"
            );
            let resolved = resolved_path(rule.path());
            assert!(
                sbpl.contains(&format!("\"{}\"", resolved.display())),
                "{name}: seatbelt dropped {path}"
            );
        }
    }
}

#[test]
fn a_denied_path_is_denied_by_both() {
    for name in ["minimal", "development", "strict"] {
        let policy = resolved(name);
        let sbpl = compile_profile(&policy);
        let bwrap = compile_filesystem_args(&policy, &AllPresent);

        for rule in policy.rules().iter().filter(|r| r.access == Access::None) {
            let path = rule.path().to_string_lossy().to_string();
            let resolved = resolved_path(rule.path());
            assert!(
                sbpl.contains(&format!(
                    "(deny file-read* file-write* (subpath \"{}\"))",
                    resolved.display()
                )),
                "{name}: seatbelt does not deny {path}"
            );
            // bwrap replaces rather than denies; either verb is a mask.
            let index = bwrap.iter().position(|arg| arg == &path).expect("emitted");
            let verb = &bwrap[index.saturating_sub(2)..index];
            assert!(
                verb.contains(&"--tmpfs".to_string()) || verb.contains(&"/dev/null".to_string()),
                "{name}: bwrap does not mask {path}, got {verb:?}"
            );
        }
    }
}

#[test]
fn strict_on_seatbelt_fails_closed_by_default() {
    // strict exists to contain untrusted work, and Seatbelt cannot mask a
    // directory or isolate the network. Under the default reaction that must
    // refuse rather than run weaker than asked.
    let policy = resolved("strict");
    let report = check_policy(
        &policy,
        RequiredGuarantees {
            network_isolation: true,
            pid_isolation: true,
            create_missing_targets: true,
        },
        &BackendCapabilities::seatbelt(),
    );
    assert!(!report.is_complete());
    assert!(authorise(report.clone(), OnMissingCapability::Fail).is_err());

    // Under warn it proceeds, and every gap is named for a person to read.
    let accepted = authorise(report.clone(), OnMissingCapability::Warn).expect("warn proceeds");
    assert!(
        accepted
            .iter()
            .any(|gap| gap.guarantee == Guarantee::NetworkIsolation)
    );
    assert_eq!(report.describe().len(), report.gaps.len());
}

#[test]
fn strict_on_bwrap_needs_no_concessions() {
    let report = check_policy(
        &resolved("strict"),
        RequiredGuarantees {
            network_isolation: true,
            pid_isolation: true,
            create_missing_targets: true,
        },
        &BackendCapabilities::bwrap(),
    );
    assert!(report.is_complete(), "unexpected gaps: {:?}", report.gaps);
}
