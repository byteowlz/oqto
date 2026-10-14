//! Release identity and ordering policy for `oqto-setup install`.
//!
//! An artifact is only installable when its file name, its manifest version
//! and its recorded build source agree, and when it is strictly newer than the
//! active release. Anything older, equal, dirty or unverifiable is refused
//! unless the operator explicitly opts in, so a stale or ambiguous checkout can
//! never silently replace a qualified installation.

use anyhow::{Context, Result};
use semver::Version;
use std::fs;
use std::path::Path;

/// Operator overrides for the release ordering policy.
#[derive(Clone, Copy, Debug, Default)]
pub struct InstallGuards {
    /// Permit a release that is not strictly newer than the active one, or
    /// whose predecessor version cannot be determined.
    pub allow_downgrade: bool,
    /// Permit a release built from a worktree with uncommitted changes.
    pub allow_dirty_source: bool,
}

/// Build provenance recorded in a staged release manifest by `package.sh`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseProvenance {
    pub version: Version,
    pub source_commit: String,
    pub source_dirty: bool,
}

/// `oqto-[v]<version>-<target>.tar.gz` -> `<version>`; the target suffix is the
/// already-validated host triple. Tag-built releases carry the `v` prefix.
pub fn version_from_artifact_name(file_name: &str, target: &str) -> Result<Version> {
    let version = file_name
        .strip_prefix("oqto-")
        .and_then(|rest| rest.strip_suffix(&format!("-{target}.tar.gz")))
        .context("artifact name must be oqto-<version>-<target>.tar.gz")?;
    let version = version.strip_prefix('v').unwrap_or(version);
    Version::parse(version)
        .with_context(|| format!("artifact version is not semantic: {version:?}"))
}

/// Read `[release] version`, `source_commit` and `source_dirty`.
pub fn read_release_provenance(release_dir: &Path) -> Result<ReleaseProvenance> {
    let path = release_dir.join("manifest.toml");
    let contents =
        fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let manifest: toml::Value = contents.parse().context("Invalid release manifest TOML")?;
    let release = manifest
        .get("release")
        .context("release manifest has no [release] table")?;
    let version = release
        .get("version")
        .and_then(toml::Value::as_str)
        .context("unversioned release: manifest lacks [release] version")?;
    let version = Version::parse(version)
        .with_context(|| format!("manifest version is not semantic: {version:?}"))?;
    let source_commit = release
        .get("source_commit")
        .and_then(toml::Value::as_str)
        .filter(|commit| commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit()))
        .context("release manifest lacks a full source_commit")?
        .to_string();
    let source_dirty = release
        .get("source_dirty")
        .and_then(toml::Value::as_bool)
        .context("release manifest lacks source_dirty provenance")?;
    Ok(ReleaseProvenance {
        version,
        source_commit,
        source_dirty,
    })
}

/// Version of an installed release directory: its manifest when versioned,
/// otherwise the `oqto-[v]<semver>-...` directory name used by releases that
/// predate recorded provenance. `None` means the version is unknowable.
pub fn installed_release_version(release_dir: &Path) -> Option<Version> {
    if let Ok(provenance) = read_release_provenance(release_dir) {
        return Some(provenance.version);
    }
    let name = release_dir.file_name()?.to_str()?;
    let rest = name.strip_prefix("oqto-")?;
    let rest = rest.strip_prefix('v').unwrap_or(rest);
    let core: String = rest
        .split('-')
        .next()
        .filter(|core| core.split('.').count() == 3)?
        .to_string();
    Version::parse(&core).ok()
}

/// Refuse dirty sources, mismatched identities and non-upgrades.
pub fn check_installable(
    named: &Version,
    staged: &ReleaseProvenance,
    active: Option<&Path>,
    guards: InstallGuards,
) -> Result<()> {
    if &staged.version != named {
        anyhow::bail!(
            "ambiguous artifact: file name declares {named} but manifest declares {}",
            staged.version
        );
    }
    if staged.source_dirty && !guards.allow_dirty_source {
        anyhow::bail!(
            "refusing release {} built from a dirty worktree at {}; rebuild from a clean commit \
             or pass --allow-dirty-source",
            staged.version,
            staged.source_commit
        );
    }
    check_order(named, active, guards)
}

/// Dev builds (`just deploy --dev`) are `<core>-dev.<timestamp>.g<sha>[.dirty]`.
pub fn is_dev_build(version: &Version) -> bool {
    version.pre.as_str().starts_with("dev.")
}

/// Semver orders `X-dev.*` below release `X`, but a dev build is made from a
/// tree already declaring `X`, i.e. after (or while preparing) that release:
/// it may replace the released `X`. Dev-over-dev is plain semver (timestamp).
fn is_dev_build_over_its_release(candidate: &Version, current: &Version) -> bool {
    is_dev_build(candidate)
        && current.pre.is_empty()
        && (candidate.major, candidate.minor, candidate.patch)
            == (current.major, current.minor, current.patch)
}

/// Strictly-newer ordering against the active release, before extraction.
pub fn check_order(
    candidate: &Version,
    active: Option<&Path>,
    guards: InstallGuards,
) -> Result<()> {
    let Some(active) = active else {
        return Ok(());
    };
    match installed_release_version(active) {
        Some(current) if candidate > &current => Ok(()),
        Some(current) if is_dev_build_over_its_release(candidate, &current) => Ok(()),
        _ if guards.allow_downgrade => Ok(()),
        Some(current) => anyhow::bail!(
            "refusing to install {candidate}: active release {} is {current}; only strictly newer \
             releases install without --allow-downgrade",
            active.display()
        ),
        None => anyhow::bail!(
            "refusing to install {candidate}: active release {} has no verifiable version; pass \
             --allow-downgrade to acknowledge",
            active.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn release(root: &Path, name: &str, release_table: &str) -> std::path::PathBuf {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("manifest.toml"),
            format!("manifest_version = 1\nid = \"oqto-dist\"\n[release]\ntarget = \"full\"\n{release_table}"),
        )
        .unwrap();
        dir
    }

    fn versioned(root: &Path, name: &str, version: &str, dirty: bool) -> std::path::PathBuf {
        release(
            root,
            name,
            &format!(
                "version = \"{version}\"\nsource_commit = \"{COMMIT}\"\nsource_dirty = {dirty}\n"
            ),
        )
    }

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn artifact_name_must_carry_a_semantic_version() {
        let target = "x86_64-unknown-linux-gnu";
        assert_eq!(
            version_from_artifact_name("oqto-0.5.9-x86_64-unknown-linux-gnu.tar.gz", target)
                .unwrap(),
            v("0.5.9")
        );
        assert_eq!(
            version_from_artifact_name("oqto-v0.5.9-x86_64-unknown-linux-gnu.tar.gz", target)
                .unwrap(),
            v("0.5.9")
        );
        for bad in [
            "oqto-20261009120000-abc-x86_64-unknown-linux-gnu.tar.gz",
            "oqto-0.5.9-aarch64-unknown-linux-gnu.tar.gz",
            "other-0.5.9-x86_64-unknown-linux-gnu.tar.gz",
        ] {
            assert!(version_from_artifact_name(bad, target).is_err(), "{bad}");
        }
    }

    #[test]
    fn provenance_is_required_and_complete() {
        let root = tempfile::tempdir().unwrap();
        let missing = release(root.path(), "missing", "");
        assert!(read_release_provenance(&missing).is_err());
        let short = release(
            root.path(),
            "short",
            "version = \"1.0.0\"\nsource_commit = \"abc\"\nsource_dirty = false\n",
        );
        assert!(read_release_provenance(&short).is_err());
        let no_dirty = release(
            root.path(),
            "nodirty",
            &format!("version = \"1.0.0\"\nsource_commit = \"{COMMIT}\"\n"),
        );
        assert!(read_release_provenance(&no_dirty).is_err());
        let ok = versioned(root.path(), "ok", "1.0.0", false);
        assert_eq!(
            read_release_provenance(&ok).unwrap(),
            ReleaseProvenance {
                version: v("1.0.0"),
                source_commit: COMMIT.into(),
                source_dirty: false
            }
        );
    }

    #[test]
    fn legacy_release_names_still_identify_the_active_version() {
        let root = tempfile::tempdir().unwrap();
        for (name, expected) in [
            (
                "oqto-v0.5.7-pi1.1.0-x86_64-unknown-linux-gnu",
                Some("0.5.7"),
            ),
            ("oqto-0.5.8-x86_64-unknown-linux-gnu", Some("0.5.8")),
            ("20260928222431-43b89f2ed8", None),
            ("oqto-latest-x86_64-unknown-linux-gnu", None),
        ] {
            let dir = release(root.path(), name, "");
            assert_eq!(installed_release_version(&dir), expected.map(v), "{name}");
        }
    }

    #[test]
    fn only_strictly_newer_clean_matching_releases_install_by_default() {
        let root = tempfile::tempdir().unwrap();
        let active = versioned(
            root.path(),
            "oqto-0.5.8-x86_64-unknown-linux-gnu",
            "0.5.8",
            false,
        );
        let clean = |version: &str| ReleaseProvenance {
            version: v(version),
            source_commit: COMMIT.into(),
            source_dirty: false,
        };
        let strict = InstallGuards::default();

        assert!(check_installable(&v("0.5.9"), &clean("0.5.9"), Some(&active), strict).is_ok());
        assert!(check_installable(&v("0.5.9"), &clean("0.5.9"), None, strict).is_ok());
        for older_or_equal in ["0.5.8", "0.5.7", "0.5.9-rc.1"] {
            let candidate = v(older_or_equal);
            let refused =
                check_installable(&candidate, &clean(older_or_equal), Some(&active), strict);
            if candidate > v("0.5.8") {
                assert!(refused.is_ok(), "{older_or_equal}");
            } else {
                assert!(refused.is_err(), "{older_or_equal} must not replace 0.5.8");
            }
        }
        let downgrade = InstallGuards {
            allow_downgrade: true,
            ..strict
        };
        assert!(check_installable(&v("0.5.7"), &clean("0.5.7"), Some(&active), downgrade).is_ok());

        let mismatch = check_installable(&v("0.6.0"), &clean("0.5.9"), Some(&active), downgrade);
        assert!(mismatch.unwrap_err().to_string().contains("ambiguous"));

        let dirty = ReleaseProvenance {
            source_dirty: true,
            ..clean("0.6.0")
        };
        assert!(check_installable(&v("0.6.0"), &dirty, Some(&active), downgrade).is_err());
        let allow_dirty = InstallGuards {
            allow_dirty_source: true,
            ..strict
        };
        assert!(check_installable(&v("0.6.0"), &dirty, Some(&active), allow_dirty).is_ok());
    }

    #[test]
    fn dev_builds_order_by_core_then_build_time() {
        let root = tempfile::tempdir().unwrap();
        let strict = InstallGuards::default();
        let active = |name: &str, version: &str| versioned(root.path(), name, version, false);
        let release = active("oqto-0.5.11-x86_64-unknown-linux-gnu", "0.5.11");
        let dev_t1 = "0.5.11-dev.20261014061200.g794dd7b8";
        let dev_t2 = "0.5.11-dev.20261014071500.gabc12345.dirty";
        let older = active("dev-old", dev_t1);

        // A dev build replaces the release of its own core, and later dev builds
        // replace earlier ones.
        assert!(check_order(&v(dev_t1), Some(&release), strict).is_ok());
        assert!(check_order(&v(dev_t2), Some(&older), strict).is_ok());
        // But never an older dev build, an older core, or a newer release.
        let newer = active("dev-new", dev_t2);
        assert!(check_order(&v(dev_t1), Some(&newer), strict).is_err());
        assert!(
            check_order(
                &v("0.5.10-dev.20261014071500.g794dd7b8"),
                Some(&release),
                strict
            )
            .is_err()
        );
        let next = active("oqto-0.5.12-x86_64-unknown-linux-gnu", "0.5.12");
        assert!(check_order(&v(dev_t2), Some(&next), strict).is_err());
        // The release itself still replaces its dev builds.
        assert!(check_order(&v("0.5.11"), Some(&newer), strict).is_ok());
        // An ordinary prerelease is not a dev build.
        assert!(check_order(&v("0.5.11-rc.1"), Some(&release), strict).is_err());
    }

    #[test]
    fn unverifiable_predecessor_requires_explicit_acknowledgement() {
        let root = tempfile::tempdir().unwrap();
        let legacy = release(root.path(), "20260928222431-43b89f2ed8", "");
        assert!(check_order(&v("9.9.9"), Some(&legacy), InstallGuards::default()).is_err());
        let acknowledged = InstallGuards {
            allow_downgrade: true,
            ..InstallGuards::default()
        };
        assert!(check_order(&v("9.9.9"), Some(&legacy), acknowledged).is_ok());
    }
}
