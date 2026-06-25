//! Typed resolver for the binary-acquisition set (ADR-0018 / vemr.9).
//!
//! Encodes the GitHub-release artifact naming that the Docker downloader stage
//! and `scripts/install.sh` already rely on, so the four duplicate acquisition
//! paths (install.sh, docker downloader, setup module 08, deploy remediate) can
//! collapse onto one manifest-driven resolver. This module is pure — parsing and
//! URL construction only, no IO — so the naming scheme is unit-testable.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// CPU architecture an artifact bundle targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
}

/// Per-arch artifact identifiers: the Rust target triple plus the goreleaser
/// OS/arch tokens used by the Go tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetTriple {
    pub rust_target: &'static str,
    pub go_os: &'static str,
    pub go_arch: &'static str,
}

impl Arch {
    pub fn target(self) -> TargetTriple {
        match self {
            Arch::X86_64 => TargetTriple {
                rust_target: "x86_64-unknown-linux-gnu",
                go_os: "Linux",
                go_arch: "x86_64",
            },
            Arch::Aarch64 => TargetTriple {
                rust_target: "aarch64-unknown-linux-gnu",
                go_os: "Linux",
                go_arch: "arm64",
            },
        }
    }
}

/// How a component's release artifact is named on GitHub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactNaming {
    /// `{name}-v{ver}-{rust_target}.tar.gz` — byteowlz Rust tools.
    RustTarget,
    /// `{name}_{GO_OS}_{GO_ARCH}.tar.gz` — goreleaser Go tools (sx, scrpr).
    GoReleaser,
    /// `oqto-v{ver}-{rust_target}.tar.gz` — the platform bundle.
    Oqto,
}

/// A pinned component to acquire from a release artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    pub name: String,
    pub version: String,
    pub naming: ArtifactNaming,
}

/// Classify a component name into its release-artifact naming scheme. Mirrors
/// the Rust/Go split in deploy.sh `dep_install_meta` and the Docker downloader.
pub fn classify(name: &str) -> ArtifactNaming {
    match name {
        "oqto" => ArtifactNaming::Oqto,
        // Go tools published via goreleaser.
        "sx" | "scrpr" => ArtifactNaming::GoReleaser,
        _ => ArtifactNaming::RustTarget,
    }
}

impl Component {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        let name = name.into();
        let naming = classify(&name);
        Self {
            name,
            version: version.into(),
            naming,
        }
    }

    /// The GitHub release download URL for this component on `target`, under the
    /// `base` org URL (e.g. `https://github.com/byteowlz`).
    pub fn artifact_url(&self, base: &str, target: TargetTriple) -> String {
        let base = base.trim_end_matches('/');
        let tag = format!("v{}", self.version);
        match self.naming {
            ArtifactNaming::Oqto => format!(
                "{base}/oqto/releases/download/{tag}/oqto-{tag}-{rt}.tar.gz",
                rt = target.rust_target
            ),
            ArtifactNaming::RustTarget => format!(
                "{base}/{name}/releases/download/{tag}/{name}-{tag}-{rt}.tar.gz",
                name = self.name,
                rt = target.rust_target
            ),
            ArtifactNaming::GoReleaser => format!(
                "{base}/{name}/releases/download/{tag}/{name}_{os}_{arch}.tar.gz",
                name = self.name,
                os = target.go_os,
                arch = target.go_arch
            ),
        }
    }
}

/// A fully-resolved artifact to fetch: where to download it, where to get its
/// checksum, and the on-disk filename to stage it under. This is the descriptor
/// a download driver consumes — the single shape the duplicate acquisition paths
/// converge on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ArtifactRef {
    pub name: String,
    pub version: String,
    pub filename: String,
    pub url: String,
    pub checksum_url: String,
}

/// Resolve every component into a concrete [`ArtifactRef`] for `target` under
/// the `base` org URL. The checksum is the conventional `{artifact}.sha256`
/// sibling (the same convention `scripts/install.sh` uses for `oqto-setup`);
/// publishing those siblings for the whole bundle is the release.yml half of
/// vemr.9.
pub fn plan_downloads(
    components: &[Component],
    base: &str,
    target: TargetTriple,
) -> Vec<ArtifactRef> {
    components
        .iter()
        .map(|c| {
            let url = c.artifact_url(base, target);
            let filename = url
                .rsplit('/')
                .next()
                .unwrap_or(c.name.as_str())
                .to_string();
            let checksum_url = format!("{url}.sha256");
            ArtifactRef {
                name: c.name.clone(),
                version: c.version.clone(),
                filename,
                url,
                checksum_url,
            }
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct RawManifest {
    oqto: OqtoSection,
    #[serde(default)]
    byteowlz: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct OqtoSection {
    version: String,
}

/// Parse `dependencies.toml` into the ordered acquisition set: the oqto platform
/// bundle first, then each pinned byteowlz tool (alphabetical, deterministic).
///
/// `[external]` tools (pi/typst/...) have their own acquisition channels (npm,
/// upstream releases) and are intentionally excluded from the byteowlz bundle.
pub fn parse_dependency_manifest(toml_str: &str) -> Result<Vec<Component>> {
    let raw: RawManifest =
        toml::from_str(toml_str).context("Failed to parse dependency manifest")?;
    let mut components = Vec::with_capacity(raw.byteowlz.len() + 1);
    components.push(Component::new("oqto", raw.oqto.version));
    for (name, version) in raw.byteowlz {
        components.push(Component::new(name, version));
    }
    Ok(components)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_naming_schemes() {
        assert_eq!(classify("oqto"), ArtifactNaming::Oqto);
        assert_eq!(classify("sx"), ArtifactNaming::GoReleaser);
        assert_eq!(classify("scrpr"), ArtifactNaming::GoReleaser);
        assert_eq!(classify("mmry"), ArtifactNaming::RustTarget);
        assert_eq!(classify("eaRS"), ArtifactNaming::RustTarget);
    }

    #[test]
    fn builds_rust_tool_url() {
        let c = Component::new("mmry", "0.11.0");
        assert_eq!(
            c.artifact_url("https://github.com/byteowlz", Arch::X86_64.target()),
            "https://github.com/byteowlz/mmry/releases/download/v0.11.0/mmry-v0.11.0-x86_64-unknown-linux-gnu.tar.gz"
        );
    }

    #[test]
    fn builds_go_tool_url_and_trims_trailing_slash() {
        let c = Component::new("sx", "2.4.0");
        assert_eq!(
            c.artifact_url("https://github.com/byteowlz/", Arch::X86_64.target()),
            "https://github.com/byteowlz/sx/releases/download/v2.4.0/sx_Linux_x86_64.tar.gz"
        );
    }

    #[test]
    fn builds_oqto_bundle_url_for_arm() {
        let c = Component::new("oqto", "0.4.0");
        assert_eq!(
            c.artifact_url("https://github.com/byteowlz", Arch::Aarch64.target()),
            "https://github.com/byteowlz/oqto/releases/download/v0.4.0/oqto-v0.4.0-aarch64-unknown-linux-gnu.tar.gz"
        );
    }

    #[test]
    fn parses_manifest_oqto_first_then_sorted_byteowlz_excluding_external() {
        let toml = r#"
[oqto]
version = "0.4.0"

[byteowlz]
mmry = "0.11.0"
trx = "0.6.1"
sx = "2.4.0"

[external]
pi = "latest"
"#;
        let comps = parse_dependency_manifest(toml).unwrap();
        assert_eq!(comps[0], Component::new("oqto", "0.4.0"));
        let names: Vec<_> = comps[1..].iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["mmry", "sx", "trx"], "byteowlz tools sorted");
        assert!(
            comps.iter().all(|c| c.name != "pi"),
            "[external] tools are excluded"
        );
        // sx classified as a Go artifact even when interleaved.
        let sx = comps.iter().find(|c| c.name == "sx").unwrap();
        assert_eq!(sx.naming, ArtifactNaming::GoReleaser);
    }

    #[test]
    fn plan_downloads_resolves_filename_and_checksum_sibling() {
        let components = vec![
            Component::new("mmry", "0.11.0"),
            Component::new("sx", "2.4.0"),
        ];
        let plan = plan_downloads(
            &components,
            "https://github.com/byteowlz",
            Arch::X86_64.target(),
        );

        assert_eq!(
            plan[0],
            ArtifactRef {
                name: "mmry".into(),
                version: "0.11.0".into(),
                filename: "mmry-v0.11.0-x86_64-unknown-linux-gnu.tar.gz".into(),
                url: "https://github.com/byteowlz/mmry/releases/download/v0.11.0/mmry-v0.11.0-x86_64-unknown-linux-gnu.tar.gz".into(),
                checksum_url: "https://github.com/byteowlz/mmry/releases/download/v0.11.0/mmry-v0.11.0-x86_64-unknown-linux-gnu.tar.gz.sha256".into(),
            }
        );
        // Go tool keeps its goreleaser filename.
        assert_eq!(plan[1].filename, "sx_Linux_x86_64.tar.gz");
        assert!(
            plan[1]
                .checksum_url
                .ends_with("sx_Linux_x86_64.tar.gz.sha256")
        );
    }
}
