//! Compile a resolved ADR-0028 policy into a macOS Seatbelt profile.
//!
//! Seatbelt is a path filter rather than a namespace, so it expresses access
//! but not masking: a denied directory becomes unreadable while its name stays
//! observable. That difference is declared in
//! [`crate::capability::BackendCapabilities::seatbelt`] and reported through
//! the capability contract instead of being papered over here.
//!
//! Like the bwrap adapter this one adds no precedence of its own. SBPL applies
//! the *last* matching rule, so emitting parent-first reproduces exactly the
//! most-specific-wins resolution the policy already decided.

use crate::path_policy::{Access, ResolvedPolicy, ResolvedRule};

/// Render a policy as an SBPL profile for `sandbox-exec`.
#[must_use]
pub fn compile_profile(policy: &ResolvedPolicy) -> String {
    let mut out = String::from("(version 1)\n");
    out.push_str(";; Generated from the sandbox path policy. Do not edit.\n");

    // Everything is denied unless the policy grants it. `(deny default)` also
    // covers operations this profile says nothing about.
    out.push_str("(deny default)\n");

    // Baseline. Verified by execution on macOS 26: without it a process dies
    // with SIGABRT before main, because dyld cannot load libraries and cannot
    // stat the directories on the way to them. `file-read-metadata` is the
    // load-bearing one: subpath grants cover descendants, but traversal still
    // needs metadata on every ancestor including "/".
    out.push_str("(allow process*)\n");
    out.push_str("(allow sysctl*)\n");
    out.push_str("(allow mach*)\n");
    out.push_str("(allow signal)\n");
    out.push_str("(allow file-read-metadata)\n");
    out.push_str(
        "(allow file-read* (subpath \"/usr\") (subpath \"/bin\") (subpath \"/System\") \
         (subpath \"/dev\") (subpath \"/private/var/db\") (subpath \"/Library\") \
         (literal \"/\"))\n",
    );

    match policy.default {
        Access::None => {}
        Access::Read => out.push_str("(allow file-read*)\n"),
        Access::Write => out.push_str("(allow file-read* file-write*)\n"),
    }

    let mut rules: Vec<&ResolvedRule> = policy.rules().iter().collect();
    // Parent-first, because SBPL takes the last match. Stable, so equal-depth
    // rules keep the order the policy was built in.
    rules.sort_by_key(|rule| rule.path().components().count());

    for rule in rules {
        // SBPL matches the *resolved* path. On macOS /tmp, /var and /etc are
        // symlinks into /private, so a rule written against the symlink never
        // matches and the path is silently denied instead of granted. Resolve
        // before emitting; fall back to the literal path when the target does
        // not exist yet, which is the only case where nothing can be resolved.
        let resolved =
            std::fs::canonicalize(rule.path()).unwrap_or_else(|_| rule.path().to_path_buf());
        let path = escape(&resolved.to_string_lossy());
        let line = match rule.access {
            Access::None => format!("(deny file-read* file-write* (subpath \"{path}\"))\n"),
            Access::Read => format!(
                "(allow file-read* (subpath \"{path}\"))\n\
                 (deny file-write* (subpath \"{path}\"))\n"
            ),
            Access::Write => {
                format!("(allow file-read* file-write* (subpath \"{path}\"))\n")
            }
        };
        out.push_str(&line);
    }
    out
}

/// Escape a path for an SBPL string literal.
///
/// A path containing a quote or backslash would otherwise end the literal early
/// and silently change which paths the rule covers.
fn escape(path: &str) -> String {
    path.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path_policy::{PolicyLayer, ResolvedRule, RuleOrigin};
    use std::path::Path;

    fn origin() -> RuleOrigin {
        RuleOrigin {
            layer: PolicyLayer::Admin,
            source: "test".to_string(),
        }
    }

    fn policy(default: Access, rules: &[(&str, Access)]) -> ResolvedPolicy {
        let mut policy = ResolvedPolicy::new(default, origin());
        for (path, access) in rules {
            policy.add_rule(ResolvedRule::new(*path, *access, origin()).expect("valid rule"));
        }
        policy
    }

    #[test]
    fn an_allowlist_policy_denies_by_default() {
        let profile = compile_profile(&policy(Access::None, &[]));
        assert!(profile.contains("(deny default)"));
        assert!(!profile.contains("(allow file-read*)\n"));
    }

    #[test]
    fn the_baseline_lets_a_process_start() {
        // Verified on macOS 26: without metadata traversal a process aborts in
        // dyld before reaching main, so every probe reports "denied" and a
        // broken profile looks like a working one.
        let profile = compile_profile(&policy(Access::None, &[]));
        assert!(profile.contains("(allow file-read-metadata)"));
        assert!(profile.contains("(literal \"/\")"));
        assert!(profile.contains("(subpath \"/usr\")"));
    }

    #[test]
    fn emitted_paths_are_resolved_not_symlinked() {
        // /tmp is a symlink to /private/tmp on macOS. Emitting the symlink
        // silently denies the path, because SBPL matches the resolved form.
        let temp = std::env::temp_dir();
        let Ok(canonical) = std::fs::canonicalize(&temp) else {
            return;
        };
        if canonical == temp {
            return; // Linux: nothing to resolve, the check is macOS-specific.
        }
        let profile = compile_profile(&policy(
            Access::None,
            &[(temp.to_str().expect("utf8"), Access::Read)],
        ));
        assert!(
            profile.contains(&format!("\"{}\"", canonical.display())),
            "profile must reference the resolved path:\n{profile}"
        );
    }

    #[test]
    fn access_levels_map_to_sbpl_verbs() {
        let profile = compile_profile(&policy(
            Access::None,
            &[
                ("/work", Access::Write),
                ("/ro", Access::Read),
                ("/secret", Access::None),
            ],
        ));
        assert!(profile.contains("(allow file-read* file-write* (subpath \"/work\"))"));
        assert!(profile.contains("(allow file-read* (subpath \"/ro\"))"));
        assert!(profile.contains("(deny file-write* (subpath \"/ro\"))"));
        assert!(profile.contains("(deny file-read* file-write* (subpath \"/secret\"))"));
    }

    #[test]
    fn deeper_rules_are_emitted_last_because_sbpl_takes_the_last_match() {
        // Built child-first: the adapter must not depend on assembly order.
        let profile = compile_profile(&policy(
            Access::None,
            &[
                ("/home/agent/.config/git", Access::Read),
                ("/home/agent/.config", Access::None),
                ("/home/agent", Access::Read),
            ],
        ));
        let home = profile.find("\"/home/agent\"").expect("home rule");
        let config = profile
            .find("\"/home/agent/.config\"")
            .expect("config rule");
        let git = profile
            .find("\"/home/agent/.config/git\"")
            .expect("git rule");
        assert!(home < config && config < git, "profile:\n{profile}");
    }

    #[test]
    fn a_path_containing_a_quote_cannot_escape_its_rule() {
        let profile = compile_profile(&policy(Access::None, &[("/work/od\"d", Access::Read)]));
        assert!(
            profile.contains(r#"(subpath "/work/od\"d")"#),
            "profile:\n{profile}"
        );
    }

    #[test]
    fn both_backends_agree_on_the_same_policy() {
        // The point of the contract: two adapters must not disagree about the
        // effective access of a path. Compare their emitted decisions per path
        // rather than their syntax.
        let resolved = policy(
            Access::None,
            &[
                ("/home/agent", Access::Read),
                ("/home/agent/.ssh", Access::None),
                ("/home/agent/work", Access::Write),
            ],
        );
        let sbpl = compile_profile(&resolved);
        for (path, expected) in [
            ("/home/agent/file", Access::Read),
            ("/home/agent/.ssh/key", Access::None),
            ("/home/agent/work/out", Access::Write),
        ] {
            assert_eq!(
                resolved.resolve(Path::new(path)).expect("resolves").access,
                expected
            );
        }
        assert!(sbpl.contains("(deny file-read* file-write* (subpath \"/home/agent/.ssh\"))"));
        assert!(sbpl.contains("(allow file-read* file-write* (subpath \"/home/agent/work\"))"));
    }
}
