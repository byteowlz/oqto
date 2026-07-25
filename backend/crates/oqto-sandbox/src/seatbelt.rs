//! Seatbelt (macOS sandbox) profile generation.
//!
//! The macOS backend enforces policy by handing `sandbox-exec` a compiled SBPL
//! profile. Profile generation is pure string building with no macOS-only APIs,
//! so it is compiled and unit-tested on every platform; only the `sandbox-exec`
//! invocation itself is macOS-gated. Keeping it that way matters because the
//! previous implementation lived in nested functions inside a
//! `cfg(target_os = "macos")` block, so no Linux build ever type-checked it.
//!
//! Scope of the guarantee: Seatbelt here protects a single-tenant workstation
//! against accidental or agent-initiated writes outside the workspace. It is
//! not a hostile multi-tenant boundary; `sandbox-exec` is a deprecated,
//! undocumented Apple interface. Multi-tenant isolation on macOS is expected to
//! run the Workspace container through Podman Machine instead (ADR-0020).

use std::path::Path;

use crate::config::SandboxConfig;

/// Device nodes that ordinary tooling expects to be writable.
///
/// Mirrors the Linux backend: `git` opens `/dev/null` read-write and shells
/// redirect to it constantly, so denying it breaks common commands.
const ALWAYS_WRITABLE_DEVICES: &[&str] = &[
    "/dev/null",
    "/dev/zero",
    "/dev/random",
    "/dev/urandom",
    "/dev/tty",
    "/dev/dtracehelper",
];

/// Escape a path for use inside an SBPL string literal.
///
/// SBPL strings are double-quoted, so a path containing `"` or `\` would
/// otherwise terminate the literal early and change the meaning of the profile
/// — a policy-injection risk for attacker-influenced workspace paths.
fn escape(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for ch in path.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

fn rule(out: &mut String, verb: &str, action: &str, kind: &str, path: &str) {
    out.push_str(&format!(
        "({} {} ({} \"{}\"))\n",
        verb,
        action,
        kind,
        escape(path)
    ));
}

/// Whether a path should be matched as a single file rather than a subtree.
fn path_kind(path: &Path) -> &'static str {
    // `literal` matches exactly one path; `subpath` matches a whole subtree.
    // Only an existing regular file is treated as a single path: a configured
    // directory that has not been created yet must still grant its subtree,
    // otherwise the rule covers just the missing entry and every write beneath
    // it is denied.
    if path.is_file() { "literal" } else { "subpath" }
}

/// Paths to emit for one configured entry: the expanded path, plus its
/// symlink-resolved target when they differ.
///
/// Seatbelt matches resolved paths, and configured entries are routinely
/// symlinks (`~/.pi` into a dotfiles tree, `/tmp` on macOS resolving to
/// `/private/tmp`). Emitting only the unresolved path silently fails to match,
/// which for a deny rule means the protection is absent.
fn rule_paths(expanded: &Path) -> Vec<std::path::PathBuf> {
    let mut paths = vec![expanded.to_path_buf()];
    if let Ok(canonical) = expanded.canonicalize()
        && canonical != expanded
    {
        paths.push(canonical);
    }
    paths
}

/// Build the SBPL profile enforcing `config` for `workspace`.
///
/// Rule order is significant: in SBPL the last matching rule wins, so the broad
/// allows are emitted first and the deny rules last.
pub fn compile_profile(config: &SandboxConfig, workspace: &Path, username: Option<&str>) -> String {
    let mut p = String::new();
    p.push_str("(version 1)\n");
    p.push_str("(deny default)\n");

    // Process and IPC basics required for any useful command.
    p.push_str("(allow process-fork)\n");
    p.push_str("(allow process-exec)\n");
    p.push_str("(allow signal)\n");
    p.push_str("(allow sysctl-read)\n");
    p.push_str("(allow mach-lookup)\n");

    // Reads are broadly permitted; deny_read below carves out secrets. This
    // matches the Linux backend, where bwrap masks specific paths rather than
    // enumerating everything readable.
    p.push_str("(allow file-read*)\n");

    // Writable: the workspace itself.
    rule(
        &mut p,
        "allow",
        "file-write*",
        "subpath",
        &workspace.to_string_lossy(),
    );

    // Writable: standard device nodes.
    for dev in ALWAYS_WRITABLE_DEVICES {
        rule(&mut p, "allow", "file-write*", "literal", dev);
    }

    // Writable: configured paths. These carry `~` and must be expanded, or the
    // rule refers to a literal "~/..." path that never matches anything.
    for path in &config.allow_write {
        let expanded = SandboxConfig::expand_home_for_user(path, username);
        for target in rule_paths(&expanded) {
            rule(
                &mut p,
                "allow",
                "file-write*",
                path_kind(&target),
                &target.to_string_lossy(),
            );
        }
    }

    // Writable: the per-workspace toolchain cache, when redirection is enabled.
    if let Ok(Some(cache)) = config.workspace_cache_dir(workspace, username) {
        rule(
            &mut p,
            "allow",
            "file-write*",
            "subpath",
            &cache.to_string_lossy(),
        );
    }

    // Denies come last so they override the allows above.
    for path in &config.deny_read {
        let expanded = SandboxConfig::expand_home_for_user(path, username);
        for target in rule_paths(&expanded) {
            let kind = path_kind(&target);
            let target = target.to_string_lossy().to_string();
            rule(&mut p, "deny", "file-read*", kind, &target);
            rule(&mut p, "deny", "file-write*", kind, &target);
        }
    }
    for path in &config.deny_write {
        let expanded = SandboxConfig::expand_home_for_user(path, username);
        for target in rule_paths(&expanded) {
            rule(
                &mut p,
                "deny",
                "file-write*",
                path_kind(&target),
                &target.to_string_lossy(),
            );
        }
    }

    if config.isolate_network {
        p.push_str("(deny network*)\n");
    } else {
        p.push_str("(allow network*)\n");
    }

    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn config_with(allow_write: Vec<&str>, deny_read: Vec<&str>) -> SandboxConfig {
        let mut config = SandboxConfig::from_profile("strict");
        config.allow_write = allow_write.into_iter().map(String::from).collect();
        config.deny_read = deny_read.into_iter().map(String::from).collect();
        config.workspace_cache_enabled = false;
        config
    }

    #[test]
    fn symlinked_directories_emit_target_and_stay_subpaths() {
        // ~/.pi is commonly a symlink into a dotfiles tree, and /tmp resolves to
        // /private/tmp on macOS. Classifying a symlink as a single `literal`
        // path leaves the whole subtree unmatched: for allow_write that means
        // Pi cannot write its session files, for deny_read it means the
        // protection silently does not apply.
        let tmp = tempfile::tempdir().expect("tempdir");
        let real = tmp.path().join("real-dir");
        std::fs::create_dir_all(&real).expect("real dir");
        let link = tmp.path().join("link-dir");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");

        let mut config = config_with(vec![], vec![]);
        config.allow_write = vec![link.to_string_lossy().to_string()];

        let profile = compile_profile(&config, &PathBuf::from("/tmp/ws"), None);

        assert!(
            profile.contains(&format!("(subpath \"{}\")", link.to_string_lossy())),
            "symlink path must grant its subtree:\n{profile}"
        );
        assert!(
            profile.contains(&format!("(subpath \"{}\")", real.to_string_lossy())),
            "resolved target must also be granted:\n{profile}"
        );
        assert!(
            !profile.contains(&format!("(literal \"{}\")", link.to_string_lossy())),
            "a symlinked directory must not be emitted as a single literal path"
        );
    }

    #[test]
    fn home_relative_paths_are_expanded() {
        // A raw "~/.cargo" in the profile matches nothing on disk, so the rule
        // silently does nothing and the path is effectively unwritable.
        let config = config_with(vec!["~/.cargo"], vec!["~/.ssh"]);
        let profile = compile_profile(&config, &PathBuf::from("/tmp/ws"), None);

        assert!(
            !profile.contains("\"~/"),
            "no unexpanded ~ may reach the profile:\n{profile}"
        );
        assert!(profile.contains(".cargo"));
        assert!(profile.contains(".ssh"));
    }

    #[test]
    fn quotes_and_backslashes_are_escaped() {
        // Without escaping, a crafted path closes the SBPL string literal and
        // injects arbitrary policy.
        let config = config_with(vec!["/tmp/we\"ird"], vec![]);
        let profile = compile_profile(&config, &PathBuf::from("/tmp/ws"), None);

        assert!(
            profile.contains("/tmp/we\\\"ird"),
            "quote must be escaped:\n{profile}"
        );
        assert!(
            !profile.contains("(allow file-write* (subpath \"/tmp/we\"ird\"))"),
            "unescaped literal would break out of the string"
        );
    }

    #[test]
    fn denies_are_emitted_after_allows() {
        // SBPL applies the last matching rule, so a deny placed before a
        // broader allow would be silently overridden.
        let config = config_with(vec!["/tmp/data"], vec!["/tmp/data/secrets"]);
        let profile = compile_profile(&config, &PathBuf::from("/tmp/ws"), None);

        let allow_at = profile.find("(allow file-write* (subpath \"/tmp/data\"))");
        let deny_at = profile.find("(deny file-read*");
        assert!(allow_at.is_some() && deny_at.is_some());
        assert!(allow_at < deny_at, "deny must come last:\n{profile}");
    }

    #[test]
    fn workspace_and_devices_are_writable() {
        let config = config_with(vec![], vec![]);
        let profile = compile_profile(&config, &PathBuf::from("/tmp/ws"), None);

        assert!(profile.contains("(allow file-write* (subpath \"/tmp/ws\"))"));
        // git opens /dev/null read-write; denying it breaks ordinary commands.
        assert!(profile.contains("(allow file-write* (literal \"/dev/null\"))"));
    }

    #[test]
    fn network_isolation_is_reflected() {
        let mut config = config_with(vec![], vec![]);
        config.isolate_network = true;
        assert!(
            compile_profile(&config, &PathBuf::from("/tmp/ws"), None).contains("(deny network*)")
        );

        config.isolate_network = false;
        assert!(
            compile_profile(&config, &PathBuf::from("/tmp/ws"), None).contains("(allow network*)")
        );
    }

    #[test]
    fn profile_starts_deny_default() {
        let config = config_with(vec![], vec![]);
        let profile = compile_profile(&config, &PathBuf::from("/tmp/ws"), None);
        let head: Vec<&str> = profile.lines().take(2).collect();
        assert_eq!(head, vec!["(version 1)", "(deny default)"]);
    }
}
