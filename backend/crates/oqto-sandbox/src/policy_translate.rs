//! Translate legacy `SandboxProfile` permission fields into an ADR-0028 policy.
//!
//! This is a behaviour-preserving mapping, not a redesign. Where the legacy
//! bwrap builder decides access by profile *name*, that quirk is reproduced
//! here and marked, so removing it later is a deliberate, testable change
//! rather than an accident of translation.

use crate::{
    config::{ReadPolicy, SandboxProfile},
    path_policy::{
        Access, Policy, PolicyError, PolicyLayer, PolicyPath, PolicyRoot, PolicyRule, RootDefault,
        RuleOrigin,
    },
};
use std::path::{Path, PathBuf};

/// Directories the bwrap builder binds read-only for every profile.
///
/// Kept in step with `SandboxConfig::build_bwrap_args_for_user`; a divergence
/// shows up as a disagreement in the differential test against the real
/// builder rather than as a silently narrower policy.
const SYSTEM_BASELINE: &[&str] = &[
    "/usr",
    "/lib",
    "/lib64",
    "/bin",
    "/sbin",
    "/etc",
    "/var/lib/oqto/pi-runtimes",
    "/run/systemd/resolve",
];

/// A legacy path string after `~/` has been split from its remainder.
enum LegacyTarget {
    Home(PathBuf),
    Absolute(PathBuf),
}

fn split_legacy_path(path: &str) -> LegacyTarget {
    match path.strip_prefix("~/") {
        Some(rest) => LegacyTarget::Home(PathBuf::from(rest)),
        None => LegacyTarget::Absolute(PathBuf::from(path)),
    }
}

/// Absolute legacy paths outside home are declared as system roots so they can
/// carry rules without inventing a filesystem-wide default.
pub struct TranslatedPolicy {
    pub policy: Policy,
    /// Absolute non-home paths the profile references, in declaration order.
    pub system_paths: Vec<PathBuf>,
}

/// Translate one profile's permission fields.
///
/// `profile_name` is required because the legacy builder binds home writable
/// for `minimal` and `development` only.
pub fn translate_profile(
    profile_name: &str,
    profile: &SandboxProfile,
) -> Result<TranslatedPolicy, PolicyError> {
    let source = format!("profile:{profile_name}");
    let origin = RuleOrigin {
        layer: PolicyLayer::Admin,
        source: source.clone(),
    };

    // Outside every declared root the answer is "no", not "inherit".
    let mut policy = Policy::new(Access::None, origin.clone());

    policy.add_root_default(RootDefault {
        root: PolicyRoot::Home,
        access: home_default_access(profile_name, profile),
        origin: RuleOrigin {
            layer: PolicyLayer::Admin,
            source: format!("{source}:home-default"),
        },
    });
    policy.add_root_default(RootDefault {
        root: PolicyRoot::Workdir,
        access: Access::Write,
        origin: RuleOrigin {
            layer: PolicyLayer::Admin,
            source: format!("{source}:workdir-default"),
        },
    });

    let mut system_paths = Vec::new();

    // The bwrap builder binds these read-only regardless of profile. They are
    // access decisions, so a policy that omits them does not describe
    // enforcement: swapping the builder for a policy-driven one would remove
    // /usr and nothing would execute. Declared first so that a profile's deny
    // of a nested path (/etc/oqto) reuses the enclosing root and stays more
    // specific than this grant.
    for path in SYSTEM_BASELINE {
        let root = PathBuf::from(path);
        system_paths.push(root.clone());
        policy.add_root_default(RootDefault {
            root: PolicyRoot::System(root),
            access: Access::Read,
            origin: RuleOrigin {
                layer: PolicyLayer::System,
                source: "builder:system-baseline".to_string(),
            },
        });
    }

    let mut pending: Vec<PendingRule> = Vec::new();

    let groups: [(&[String], LegacyGrant, &str); 6] = [
        (&profile.allow_read, LegacyGrant::Read, "allow_read"),
        (&profile.extra_ro_bind, LegacyGrant::Read, "extra_ro_bind"),
        (&profile.allow_write, LegacyGrant::Write, "allow_write"),
        (&profile.extra_rw_bind, LegacyGrant::Write, "extra_rw_bind"),
        (&profile.deny_write, LegacyGrant::DenyWrite, "deny_write"),
        (&profile.deny_read, LegacyGrant::DenyRead, "deny_read"),
    ];

    for (paths, grant, field) in groups {
        for path in paths {
            let (root, relative) = match split_legacy_path(path) {
                LegacyTarget::Home(relative) => (PolicyRoot::Home, relative),
                LegacyTarget::Absolute(absolute) => {
                    let root = system_root_for(&absolute, &mut system_paths);
                    let relative = absolute
                        .strip_prefix(&root)
                        .unwrap_or(Path::new(""))
                        .to_path_buf();
                    (PolicyRoot::System(root), relative)
                }
            };

            match pending
                .iter_mut()
                .find(|entry| entry.root == root && entry.relative == relative)
            {
                Some(entry) => entry.apply(grant, field),
                None => {
                    let mut entry = PendingRule {
                        root,
                        relative,
                        grant: LegacyGrant::None,
                        fields: Vec::new(),
                    };
                    entry.apply(grant, field);
                    pending.push(entry);
                }
            }
        }
    }

    for entry in pending {
        let Some(access) = entry.grant.access() else {
            continue;
        };
        policy.add_rule(PolicyRule {
            target: PolicyPath::new(entry.root, entry.relative)?,
            access,
            origin: RuleOrigin {
                layer: PolicyLayer::Admin,
                source: format!("{source}:{}", entry.fields.join("+")),
            },
        });
    }

    Ok(TranslatedPolicy {
        policy,
        system_paths,
    })
}

/// The legacy field kinds that can name the same path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LegacyGrant {
    None,
    Read,
    Write,
    DenyWrite,
    DenyRead,
}

impl LegacyGrant {
    /// Combine two legacy fields naming one path.
    ///
    /// The bwrap builder resolves this by emission order: a later `--bind`
    /// replaces an earlier `--ro-bind`, which is why `strict` can list `~/.pi`
    /// as both readable and writable and get a writable bind. Resolution here
    /// is order-independent and ties resolve to the lower access, so duplicates
    /// are merged before a rule is emitted rather than left to collide.
    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::DenyRead, _) | (_, Self::DenyRead) => Self::DenyRead,
            (Self::DenyWrite, _) | (_, Self::DenyWrite) => Self::DenyWrite,
            (Self::Write, _) | (_, Self::Write) => Self::Write,
            (Self::Read, _) | (_, Self::Read) => Self::Read,
            (Self::None, Self::None) => Self::None,
        }
    }

    fn access(self) -> Option<Access> {
        match self {
            Self::None => None,
            Self::DenyRead => Some(Access::None),
            // A denied write clamps an inherited grant instead of removing the
            // path; only deny_read makes it absent.
            Self::Read | Self::DenyWrite => Some(Access::Read),
            Self::Write => Some(Access::Write),
        }
    }
}

struct PendingRule {
    root: PolicyRoot,
    relative: PathBuf,
    grant: LegacyGrant,
    fields: Vec<&'static str>,
}

impl PendingRule {
    fn apply(&mut self, grant: LegacyGrant, field: &'static str) {
        self.grant = self.grant.combine(grant);
        if !self.fields.contains(&field) {
            self.fields.push(field);
        }
    }
}

/// Reproduce the legacy home binding decision.
///
/// `config.rs` computes `home_writable = profile == "development" || profile ==
/// "minimal"`, so a denylist profile under any other name binds home read-only.
/// Deriving the default from `read_policy` alone would silently change both
/// cases. ADR-0028 keeps this until each profile states its own home default.
fn home_default_access(profile_name: &str, profile: &SandboxProfile) -> Access {
    if profile.read_policy == ReadPolicy::Allowlist {
        return Access::None;
    }
    if matches!(profile_name, "development" | "minimal") {
        Access::Write
    } else {
        Access::Read
    }
}

/// Reuse an already-declared system root when one contains this path.
///
/// Without this, `/etc` and `/etc/oqto` would become sibling roots and the
/// deny of `/etc/oqto` would not be more specific than the grant of `/etc`.
fn system_root_for(path: &Path, system_paths: &mut Vec<PathBuf>) -> PathBuf {
    if let Some(existing) = system_paths
        .iter()
        .filter(|root| path.starts_with(root))
        .max_by_key(|root| root.components().count())
    {
        return existing.clone();
    }
    system_paths.push(path.to_path_buf());
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path_policy::{ResolutionContext, ResourceRegistry};

    fn resolve(translated: &TranslatedPolicy, path: &str) -> Access {
        let registry = ResourceRegistry::default();
        let context = ResolutionContext {
            workdir: Path::new("/work/project"),
            home: Path::new("/home/agent"),
            resources: &registry,
        };
        translated
            .policy
            .resolve_roots(&context)
            .expect("roots resolve")
            .policy
            .resolve(Path::new(path))
            .expect("path resolves")
            .access
    }

    #[test]
    fn development_keeps_its_writable_home() {
        let translated = translate_profile("development", &SandboxProfile::development()).unwrap();
        assert_eq!(resolve(&translated, "/home/agent/scratch"), Access::Write);
        assert_eq!(
            resolve(&translated, "/home/agent/.ssh/id_ed25519"),
            Access::None
        );
    }

    #[test]
    fn a_denylist_profile_under_another_name_keeps_read_only_home() {
        let mut profile = SandboxProfile::development();
        profile.read_policy = ReadPolicy::Denylist;
        let translated = translate_profile("team-default", &profile).unwrap();
        assert_eq!(resolve(&translated, "/home/agent/scratch"), Access::Read);
    }

    #[test]
    fn strict_masks_home_and_grants_only_enumerated_paths() {
        let translated = translate_profile("strict", &SandboxProfile::strict()).unwrap();
        assert_eq!(resolve(&translated, "/home/agent/anything"), Access::None);
        assert_eq!(
            resolve(&translated, "/home/agent/.cargo/config.toml"),
            Access::Read
        );
        assert_eq!(
            resolve(&translated, "/home/agent/.pi/sessions"),
            Access::Write
        );
    }

    #[test]
    fn strict_reaches_sandbox_config_through_a_denied_parent() {
        // Legacy relies on extra_ro_bind being emitted after the deny_read
        // tmpfs. Most-specific-match reaches the same result without ordering.
        let translated = translate_profile("strict", &SandboxProfile::strict()).unwrap();
        assert_eq!(
            resolve(&translated, "/home/agent/.config/oqto/config.toml"),
            Access::None
        );
        assert_eq!(
            resolve(&translated, "/home/agent/.config/oqto/sandbox.toml"),
            Access::Read
        );
    }

    #[test]
    fn a_path_listed_readable_and_writable_stays_writable() {
        // strict lists ~/.pi in allow_read and allow_write. The legacy builder
        // resolves that by emission order and produces a writable bind; losing
        // it makes the harness exit 0 while persisting no session.
        let translated = translate_profile("strict", &SandboxProfile::strict()).unwrap();
        assert_eq!(
            resolve(&translated, "/home/agent/.pi/agent/sessions"),
            Access::Write
        );
    }

    #[test]
    fn deny_write_leaves_a_path_readable() {
        let translated = translate_profile("development", &SandboxProfile::development()).unwrap();
        assert_eq!(
            resolve(&translated, "/home/agent/.config/oqto/sandbox.toml"),
            Access::Read
        );
    }

    #[test]
    fn nested_system_paths_share_one_root_so_denies_stay_specific() {
        let translated = translate_profile("strict", &SandboxProfile::strict()).unwrap();
        assert_eq!(resolve(&translated, "/etc/oqto/config.toml"), Access::None);
    }

    #[test]
    fn credential_paths_are_denied_in_every_builtin_profile() {
        for name in ["minimal", "development", "strict"] {
            let profile = SandboxProfile::builtin(name).expect("builtin profile");
            let translated = translate_profile(name, &profile).unwrap();
            for denied in [
                "/home/agent/.ssh/id_ed25519",
                "/home/agent/.local/share/oqto/credentials/jwt",
                "/etc/oqto/config.toml",
            ] {
                assert_eq!(
                    resolve(&translated, denied),
                    Access::None,
                    "{name} must deny {denied}"
                );
            }
        }
    }
}
