//! Typed resolver for the binary-acquisition set (ADR-0018 / ADR-0021 / vemr.9).
//!
//! Every byteowlz release (Rust and Go alike) is now packaged by `byt release`
//! to one spec (ADR-0021): `{name}-v{ver}-{target-triple}.tar.gz`, a top-level
//! `{name}-v{ver}-{triple}/bin/<exes>` layout, and a combined `checksums.txt`.
//! So acquisition collapses to a single naming rule — no per-tool special cases.
//! This module is pure (parsing + URL construction, no IO), so the scheme is
//! unit-testable; the four former acquisition paths (install.sh, docker
//! downloader, setup module 08, deploy remediate) converge on it.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// CPU architecture an artifact bundle targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
}

impl Arch {
    /// The Rust/ADR-0021 target triple for this arch (Go releases use the same
    /// triple in their artifact names — no separate goreleaser tokens anymore).
    pub fn target(self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64-unknown-linux-gnu",
            Arch::Aarch64 => "aarch64-unknown-linux-gnu",
        }
    }
}

/// A pinned component to acquire from a release artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    pub name: String,
    pub version: String,
}

impl Component {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }

    /// The GitHub release download URL for this component on `target`, under the
    /// `base` org URL (e.g. `https://github.com/byteowlz`). One rule for every
    /// tool: `{base}/{name}/releases/download/v{ver}/{name}-v{ver}-{triple}.tar.gz`.
    pub fn artifact_url(&self, base: &str, target: &str) -> String {
        let base = base.trim_end_matches('/');
        let tag = format!("v{}", self.version);
        format!(
            "{base}/{name}/releases/download/{tag}/{name}-{tag}-{target}.tar.gz",
            name = self.name,
        )
    }

    /// The combined `checksums.txt` URL for this component's release. One file
    /// per release covers every arch (`<sha256>  <filename>` lines), which is
    /// what byteowlz/oqto releases publish (no per-file `.sha256`).
    pub fn checksums_url(&self, base: &str) -> String {
        let base = base.trim_end_matches('/');
        format!(
            "{base}/{name}/releases/download/v{ver}/checksums.txt",
            name = self.name,
            ver = self.version
        )
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
    /// The combined `checksums.txt` for this artifact's release. One file per
    /// release lists every artifact as `<sha256>  <filename>` lines; the driver
    /// resolves this artifact's hash by looking up `filename`.
    pub checksums_url: String,
}

/// Resolve every component into a concrete [`ArtifactRef`] for `target` under
/// the `base` org URL, using the single ADR-0021 naming rule.
pub fn plan_downloads(components: &[Component], base: &str, target: &str) -> Vec<ArtifactRef> {
    components
        .iter()
        .map(|c| {
            let url = c.artifact_url(base, target);
            let filename = url
                .rsplit('/')
                .next()
                .unwrap_or(c.name.as_str())
                .to_string();
            let checksums_url = c.checksums_url(base);
            ArtifactRef {
                name: c.name.clone(),
                version: c.version.clone(),
                filename,
                url,
                checksums_url,
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
    fn builds_rust_tool_url() {
        let c = Component::new("mmry", "0.11.0");
        assert_eq!(
            c.artifact_url("https://github.com/byteowlz", Arch::X86_64.target()),
            "https://github.com/byteowlz/mmry/releases/download/v0.11.0/mmry-v0.11.0-x86_64-unknown-linux-gnu.tar.gz"
        );
    }

    #[test]
    fn go_tools_use_the_same_naming_rule_and_trim_trailing_slash() {
        // sx/scrpr now release via `byt release`, so they use the ADR-0021 triple
        // naming exactly like the Rust tools — no goreleaser `_Linux_x86_64` form.
        let c = Component::new("sx", "2.4.0");
        assert_eq!(
            c.artifact_url("https://github.com/byteowlz/", Arch::X86_64.target()),
            "https://github.com/byteowlz/sx/releases/download/v2.4.0/sx-v2.4.0-x86_64-unknown-linux-gnu.tar.gz"
        );
    }

    #[test]
    fn oqto_bundle_uses_the_same_rule_for_arm() {
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
    }

    #[test]
    fn plan_downloads_resolves_filename_and_checksums_url() {
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
                checksums_url: "https://github.com/byteowlz/mmry/releases/download/v0.11.0/checksums.txt".into(),
            }
        );
        // Go tool now shares the exact same filename shape as the Rust tools.
        assert_eq!(
            plan[1].filename,
            "sx-v2.4.0-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(
            plan[1].checksums_url,
            "https://github.com/byteowlz/sx/releases/download/v2.4.0/checksums.txt"
        );
    }
}
