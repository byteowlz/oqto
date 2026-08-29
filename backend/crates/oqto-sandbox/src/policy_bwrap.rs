//! Compile a resolved ADR-0028 policy into bwrap filesystem arguments.
//!
//! The neutral policy is order-independent; bwrap is not. Later mounts shadow
//! earlier ones, so this adapter is responsible for emitting rules parent-first
//! and nothing else. It must not introduce precedence of its own: two backends
//! disagreeing about the effective access of a path is an adapter bug.

use crate::path_policy::{Access, ResolvedPolicy, ResolvedRule};
use std::path::Path;

/// How a subtree reaches the namespace; decides symlink reconstruction.
#[derive(Clone, Copy, PartialEq)]
enum Subtree {
    /// The namespace root is the live host root (no root-level remount).
    Live,
    /// A bind brought the host subtree into the namespace.
    HostBound,
    /// A tmpfs replaced the subtree; host contents are absent.
    Masked,
}

/// Whether a bind source exists on this host.
///
/// Injected so the mapping can be tested without touching the filesystem.
pub trait PathPresence {
    fn exists(&self, path: &Path) -> bool;
    /// Whether the path is a regular file rather than a directory.
    fn is_file(&self, path: &Path) -> bool;
}

/// Real filesystem lookup.
pub struct HostPaths;

impl PathPresence for HostPaths {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
}

/// Compile the filesystem portion of a sandbox invocation.
///
/// Emits only access decisions. System scaffolding (`--proc`, `--dev`, DNS,
/// managed runtimes) and materialisation (overlays, caches) are separate
/// concerns and are not produced here.
pub fn compile_filesystem_args(
    policy: &ResolvedPolicy,
    presence: &dyn PathPresence,
    reconstructions: &[(std::path::PathBuf, std::path::PathBuf)],
) -> Vec<String> {
    let mut rules: Vec<&ResolvedRule> = policy.rules().iter().collect();
    // Parent-first: bwrap resolves overlapping mounts by order, so a deeper
    // rule must be applied after the rule it refines. Sorting is stable, so
    // equal-depth rules keep the order the policy was built in.
    rules.sort_by_key(|rule| rule.path().components().count());

    let mut args = Vec::new();
    // How each mounted subtree reaches the namespace: needed to decide
    // whether canonicalized paths still require a leaf symlink.
    let mut mount_stack: Vec<(std::path::PathBuf, Subtree)> = Vec::new();
    for rule in rules {
        let path = rule.path();
        let path_str = path.to_string_lossy().to_string();
        match rule.access {
            // A masked path is replaced rather than omitted: leaving it out
            // would expose whatever the surrounding bind already made visible.
            //
            // A tmpfs cannot be mounted over a regular file, so a denied file is
            // replaced by /dev/null instead. Getting this wrong aborts the spawn
            // rather than degrading quietly.
            Access::None if presence.is_file(path) => {
                args.push("--bind".to_string());
                args.push("/dev/null".to_string());
                args.push(path_str.clone());
                mount_stack.push((path.to_path_buf(), Subtree::Masked));
            }
            Access::None => {
                args.push("--tmpfs".to_string());
                args.push(path_str.clone());
                mount_stack.push((path.to_path_buf(), Subtree::Masked));
            }
            // bwrap does not create bind sources, and binding an absent path
            // aborts the whole spawn. Optional tool directories must therefore
            // be skipped rather than fail every session.
            Access::Read | Access::Write if !presence.exists(path) => {}
            Access::Read => {
                args.push("--ro-bind".to_string());
                args.push(path_str.clone());
                args.push(path_str.clone());
                mount_stack.push((path.to_path_buf(), Subtree::HostBound));
            }
            Access::Write => {
                args.push("--bind".to_string());
                args.push(path_str.clone());
                args.push(path_str.clone());
                mount_stack.push((path.to_path_buf(), Subtree::HostBound));
            }
        }
    }
    // Policy paths that were canonicalized away from a host symlink get their
    // original location re-created as a leaf symlink — but only where that
    // original path is not already visible in the namespace: under a host
    // bind (or the live root) the host symlink itself is carried there and
    // resolves through to the canonical mount, while under a tmpfs mask the
    // path would not exist at all. Nothing is ever mounted *through* these
    // links: bwrap cannot mkdir intermediate paths through a symlink.
    let nearest = |path: &std::path::Path| -> Subtree {
        let mut best: Option<(&std::path::PathBuf, Subtree)> = None;
        for (root, kind) in &mount_stack {
            if path.starts_with(root)
                && !best.is_some_and(|(best_root, _)| {
                    root.components().count() > best_root.components().count()
                })
            {
                best = Some((root, *kind));
            }
        }
        best.map(|(_, kind)| kind).unwrap_or(Subtree::Live)
    };
    let mut seen_links: Vec<std::path::PathBuf> = Vec::new();
    for (canonical, original) in reconstructions {
        if seen_links.iter().any(|link| original.starts_with(link)) {
            continue;
        }
        if nearest(original) == Subtree::Masked {
            args.push("--symlink".to_string());
            args.push(canonical.to_string_lossy().to_string());
            args.push(original.to_string_lossy().to_string());
            seen_links.push(original.to_path_buf());
        }
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SandboxProfile;
    use crate::{
        path_policy::{
            PolicyLayer, ResolutionContext, ResolvedPolicy, ResourceRegistry, RuleOrigin,
        },
        policy_translate::translate_profile,
    };
    use std::path::PathBuf;

    struct AllPresent;
    impl PathPresence for AllPresent {
        fn exists(&self, _path: &Path) -> bool {
            true
        }
        fn is_file(&self, _path: &Path) -> bool {
            false
        }
    }

    struct Files(Vec<PathBuf>);
    impl PathPresence for Files {
        fn exists(&self, _path: &Path) -> bool {
            true
        }
        fn is_file(&self, path: &Path) -> bool {
            self.0.iter().any(|file| file == path)
        }
    }

    struct Absent(Vec<PathBuf>);
    impl PathPresence for Absent {
        fn exists(&self, path: &Path) -> bool {
            !self.0.iter().any(|absent| absent == path)
        }
        fn is_file(&self, _path: &Path) -> bool {
            false
        }
    }

    fn origin() -> RuleOrigin {
        RuleOrigin {
            layer: PolicyLayer::Admin,
            source: "test".to_string(),
        }
    }

    fn policy_with(rules: &[(&str, Access)]) -> ResolvedPolicy {
        let mut policy = ResolvedPolicy::new(Access::None, origin());
        for (path, access) in rules {
            policy.add_rule(ResolvedRule::new(*path, *access, origin()).expect("valid rule"));
        }
        policy
    }

    fn index_of(args: &[String], needle: &str) -> usize {
        args.iter()
            .position(|arg| arg == needle)
            .unwrap_or_else(|| panic!("missing {needle} in {args:?}"))
    }

    #[test]
    fn access_levels_map_to_bwrap_verbs() {
        let args = compile_filesystem_args(
            &policy_with(&[
                ("/masked", Access::None),
                ("/readable", Access::Read),
                ("/writable", Access::Write),
            ]),
            &AllPresent,
            &[],
        );
        assert_eq!(
            args,
            vec![
                "--tmpfs",
                "/masked",
                "--ro-bind",
                "/readable",
                "/readable",
                "--bind",
                "/writable",
                "/writable",
            ]
        );
    }

    #[test]
    fn deeper_rules_are_emitted_after_the_rules_they_refine() {
        // Built child-first on purpose: the adapter must not depend on the
        // order the policy was assembled in.
        let args = compile_filesystem_args(
            &policy_with(&[
                ("/home/agent/.config/oqto/sandbox.toml", Access::Read),
                ("/home/agent/.config", Access::None),
                ("/home/agent", Access::Read),
            ]),
            &AllPresent,
            &[],
        );
        assert!(index_of(&args, "/home/agent") < index_of(&args, "/home/agent/.config"));
        assert!(
            index_of(&args, "/home/agent/.config")
                < index_of(&args, "/home/agent/.config/oqto/sandbox.toml")
        );
    }

    #[test]
    fn a_denied_file_is_replaced_rather_than_covered_by_a_tmpfs() {
        let args = compile_filesystem_args(
            &policy_with(&[
                ("/home/agent/.config/oqto/config.toml", Access::None),
                ("/home/agent/.ssh", Access::None),
            ]),
            &Files(vec![PathBuf::from("/home/agent/.config/oqto/config.toml")]),
            &[],
        );
        assert_eq!(
            args,
            vec![
                "--tmpfs",
                "/home/agent/.ssh",
                "--bind",
                "/dev/null",
                "/home/agent/.config/oqto/config.toml",
            ]
        );
    }

    #[test]
    fn absent_bind_sources_are_skipped_but_masks_still_apply() {
        let args = compile_filesystem_args(
            &policy_with(&[
                ("/opt/missing-tool", Access::Write),
                ("/home/agent/.ssh", Access::None),
            ]),
            &Absent(vec![
                PathBuf::from("/opt/missing-tool"),
                PathBuf::from("/home/agent/.ssh"),
            ]),
            &[],
        );
        assert_eq!(args, vec!["--tmpfs", "/home/agent/.ssh"]);
    }

    #[test]
    fn reconstructions_emit_symlinks_only_under_masked_parents() {
        // Under a tmpfs-masked parent the original path would not exist, so
        // the reconstruction is emitted; under a host-bound parent the host
        // symlink is already visible and resolves through — no extra link.
        let temp = tempfile::tempdir().expect("tempdir");
        let real = temp.path().join("stow").join(".pi");
        std::fs::create_dir_all(&real).expect("create real dir");
        let link = temp.path().join(".pi");
        std::os::unix::fs::symlink(&real, &link).expect("create symlink");

        // Masked home: tmpfs at the temp root, then a Read rule through the
        // canonical path (as the policy layer would produce).
        let masked_root = temp.path().to_path_buf();
        let mut policy = ResolvedPolicy::new(Access::None, origin());
        policy
            .add_rule(ResolvedRule::new(&masked_root, Access::None, origin()).expect("valid rule"));
        policy.add_rule(ResolvedRule::new(&real, Access::Read, origin()).expect("valid rule"));
        let args = compile_filesystem_args(&policy, &AllPresent, &[(real.clone(), link.clone())]);
        assert!(
            args.windows(2).any(|w| w == ["--symlink", "--ro-bind"])
                || args.contains(&"--symlink".to_string())
        );
        assert!(args.contains(&masked_root.to_string_lossy().to_string()));
        // no duplicate mount of the literal link path
        assert!(
            !args
                .windows(2)
                .any(|w| w == ["--bind", link.to_str().expect("utf8")])
        );

        // Host-bound parent: a bind covering the original path means the
        // symlink is visible already; reconstruction must be skipped.
        let bound_root = temp.path().join("stow");
        let mut policy = ResolvedPolicy::new(Access::None, origin());
        policy
            .add_rule(ResolvedRule::new(&bound_root, Access::Read, origin()).expect("valid rule"));
        let args = compile_filesystem_args(&policy, &AllPresent, &[(real.clone(), link)]);
        assert!(
            !args.contains(&"--symlink".to_string()),
            "reconstruction must be skipped under a host-bound parent: {args:?}"
        );
    }

    #[test]
    fn strict_masks_config_before_reopening_the_sandbox_file() {
        let translated = translate_profile("strict", &SandboxProfile::strict()).unwrap();
        let registry = ResourceRegistry::default();
        let resolved = translated
            .policy
            .resolve_roots(&ResolutionContext {
                workdir: Path::new("/work/project"),
                home: Path::new("/home/agent"),
                resources: &registry,
            })
            .expect("roots resolve")
            .policy;

        let args = compile_filesystem_args(&resolved, &AllPresent, &[]);
        assert!(
            index_of(&args, "/home/agent/.config")
                < index_of(&args, "/home/agent/.config/oqto/sandbox.toml"),
            "the sandbox file must be rebound after its parent is masked"
        );
    }
}
