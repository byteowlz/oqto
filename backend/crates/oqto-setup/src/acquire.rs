//! Manifest-driven artifact acquisition (ADR-0018 / vemr.9).
//!
//! Consumes the [`crate::deps::ArtifactRef`] plan and fetches + checksum-verifies
//! each artifact into a staging dir. Fetching is abstracted behind [`Fetcher`] so
//! the orchestration is unit-testable without network; the real impl shells out
//! to `curl` like the scripts it replaces. This is the single acquisition path
//! the four duplicate implementations converge on.

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::deps::ArtifactRef;

/// Fetch a URL to a local path. Abstracted so the driver can be tested with a
/// fake fetcher instead of real network access.
pub trait Fetcher {
    fn fetch(&self, url: &str, dest: &Path) -> Result<()>;
}

/// `curl`-based fetcher matching the existing scripts' download behavior.
pub struct CurlFetcher;

impl Fetcher for CurlFetcher {
    fn fetch(&self, url: &str, dest: &Path) -> Result<()> {
        let status = Command::new("curl")
            .args(["-fSL", "--retry", "3", "-o"])
            .arg(dest)
            .arg(url)
            .status()
            .with_context(|| format!("failed to spawn curl for {url}"))?;
        if !status.success() {
            bail!("curl failed to download {url}");
        }
        Ok(())
    }
}

/// Lowercase hex sha256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Verify `artifact` against the sha256 recorded in `checksum_file` (the first
/// whitespace token, `sha256sum` format), computing the digest in-process.
pub fn verify_sha256(artifact: &Path, checksum_file: &Path) -> Result<()> {
    let expected = std::fs::read_to_string(checksum_file)
        .with_context(|| format!("Failed reading checksum {}", checksum_file.display()))?
        .split_whitespace()
        .next()
        .map(str::to_ascii_lowercase)
        .context("Checksum file missing hash")?;

    let bytes = std::fs::read(artifact)
        .with_context(|| format!("Failed reading artifact {}", artifact.display()))?;
    let actual = sha256_hex(&bytes);

    if actual != expected {
        bail!(
            "Checksum mismatch for {} (expected {expected}, got {actual})",
            artifact.display()
        );
    }
    Ok(())
}

/// Fetch every artifact in `plan` (and its checksum sibling) into `dest_dir`,
/// verifying each against its published sha256. Returns the staged tarball paths
/// in plan order. Fail-closed: a download or checksum failure aborts the bundle.
pub fn acquire_artifacts(
    plan: &[ArtifactRef],
    dest_dir: &Path,
    fetcher: &dyn Fetcher,
) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dest_dir)
        .with_context(|| format!("Failed creating download dir {}", dest_dir.display()))?;

    let mut staged = Vec::with_capacity(plan.len());
    for artifact in plan {
        let tarball = dest_dir.join(&artifact.filename);
        fetcher
            .fetch(&artifact.url, &tarball)
            .with_context(|| format!("Failed downloading {}", artifact.name))?;

        let checksum_file = dest_dir.join(format!("{}.sha256", artifact.filename));
        fetcher
            .fetch(&artifact.checksum_url, &checksum_file)
            .with_context(|| format!("Failed downloading checksum for {}", artifact.name))?;

        verify_sha256(&tarball, &checksum_file)
            .with_context(|| format!("Checksum verification failed for {}", artifact.name))?;
        staged.push(tarball);
    }
    Ok(staged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// In-memory fetcher: serves predefined url->bytes and records call order.
    struct FakeFetcher {
        files: HashMap<String, Vec<u8>>,
        calls: RefCell<Vec<String>>,
    }

    impl Fetcher for FakeFetcher {
        fn fetch(&self, url: &str, dest: &Path) -> Result<()> {
            let bytes = self
                .files
                .get(url)
                .cloned()
                .with_context(|| format!("no fake content for {url}"))?;
            self.calls.borrow_mut().push(url.to_string());
            std::fs::write(dest, bytes)?;
            Ok(())
        }
    }

    fn artifact_ref(name: &str, url: &str, filename: &str) -> ArtifactRef {
        ArtifactRef {
            name: name.into(),
            version: "0.1.0".into(),
            filename: filename.into(),
            url: url.into(),
            checksum_url: format!("{url}.sha256"),
        }
    }

    /// Build the fake file table for one artifact whose checksum matches.
    fn good_files(url: &str, filename: &str, content: &[u8]) -> HashMap<String, Vec<u8>> {
        let mut files = HashMap::new();
        files.insert(url.to_string(), content.to_vec());
        files.insert(
            format!("{url}.sha256"),
            format!("{}  {filename}\n", sha256_hex(content)).into_bytes(),
        );
        files
    }

    #[test]
    fn sha256_hex_matches_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn acquire_fetches_and_verifies_each_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let url = "https://example/mmry.tar.gz";
        let content = b"fake-tarball-bytes";
        let fetcher = FakeFetcher {
            files: good_files(url, "mmry.tar.gz", content),
            calls: RefCell::new(Vec::new()),
        };
        let plan = vec![artifact_ref("mmry", url, "mmry.tar.gz")];

        let staged = acquire_artifacts(&plan, dir.path(), &fetcher).unwrap();

        assert_eq!(staged, vec![dir.path().join("mmry.tar.gz")]);
        assert_eq!(std::fs::read(&staged[0]).unwrap(), content);
        // Both the artifact and its checksum sibling were fetched.
        assert_eq!(
            *fetcher.calls.borrow(),
            vec![url.to_string(), format!("{url}.sha256")]
        );
    }

    #[test]
    fn acquire_rejects_checksum_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let url = "https://example/trx.tar.gz";
        // Checksum recorded for *different* bytes than what is served.
        let mut files = HashMap::new();
        files.insert(url.to_string(), b"actual-bytes".to_vec());
        files.insert(
            format!("{url}.sha256"),
            format!("{}  trx.tar.gz\n", sha256_hex(b"expected-other-bytes")).into_bytes(),
        );
        let fetcher = FakeFetcher {
            files,
            calls: RefCell::new(Vec::new()),
        };
        let plan = vec![artifact_ref("trx", url, "trx.tar.gz")];

        let err = acquire_artifacts(&plan, dir.path(), &fetcher).unwrap_err();
        assert!(
            format!("{err:#}").contains("Checksum"),
            "expected checksum failure, got: {err:#}"
        );
    }

    #[test]
    fn acquire_aborts_on_download_failure() {
        let dir = tempfile::tempdir().unwrap();
        // Empty file table -> fetch fails for the first artifact.
        let fetcher = FakeFetcher {
            files: HashMap::new(),
            calls: RefCell::new(Vec::new()),
        };
        let plan = vec![artifact_ref(
            "eavs",
            "https://example/eavs.tar.gz",
            "eavs.tar.gz",
        )];

        assert!(acquire_artifacts(&plan, dir.path(), &fetcher).is_err());
    }
}
