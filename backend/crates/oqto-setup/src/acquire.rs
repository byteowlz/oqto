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

/// Extract the lowercase sha256 recorded for `filename` from a combined
/// `checksums.txt` body (`<sha256>  <filename>` lines, optional `*` binary
/// marker). Errors if no line matches.
pub fn checksum_for(checksums: &str, filename: &str) -> Result<String> {
    for line in checksums.lines() {
        let mut parts = line.split_whitespace();
        let (Some(hash), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if name.trim_start_matches('*') == filename {
            return Ok(hash.to_ascii_lowercase());
        }
    }
    bail!("checksums.txt has no entry for {filename}")
}

/// Verify `artifact` against the sha256 recorded for `filename` in a combined
/// `checksums.txt` file — the format byteowlz/oqto releases actually publish
/// (one file per release, not a per-artifact `.sha256` sibling).
pub fn verify_against_checksums(
    artifact: &Path,
    checksums_file: &Path,
    filename: &str,
) -> Result<()> {
    let checksums = std::fs::read_to_string(checksums_file)
        .with_context(|| format!("Failed reading checksums {}", checksums_file.display()))?;
    let expected = checksum_for(&checksums, filename)
        .with_context(|| format!("in {}", checksums_file.display()))?;

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

/// Fetch every artifact in `plan` (and its release `checksums.txt`) into
/// `dest_dir`, verifying each against the published sha256. Returns the staged
/// tarball paths in plan order. Fail-closed: a download or checksum failure
/// aborts the bundle.
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

        let checksums_file = dest_dir.join(format!("{}.checksums.txt", artifact.filename));
        fetcher
            .fetch(&artifact.checksums_url, &checksums_file)
            .with_context(|| format!("Failed downloading checksums for {}", artifact.name))?;

        verify_against_checksums(&tarball, &checksums_file, &artifact.filename)
            .with_context(|| format!("Checksum verification failed for {}", artifact.name))?;
        staged.push(tarball);
    }
    Ok(staged)
}

/// Recursively collect regular files under `root`.
fn walk_files(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(root).with_context(|| format!("reading {}", root.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            walk_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}
#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("exe")
}

/// Pick which extracted files are the binaries to install: if any live under a
/// `bin/` directory (the standardized bundle layout, ADR-0021), install those;
/// otherwise fall back to every executable regular file (covers today's flat
/// byteowlz tool tarballs). `LICENSE`/`README` etc. are non-executable, skipped.
fn pick_binaries(files: &[(PathBuf, bool)]) -> Vec<PathBuf> {
    // Files directly under a `bin/` dir, paired with that bin dir's depth.
    let in_bin: Vec<(&PathBuf, usize)> = files
        .iter()
        .filter(|(p, _)| {
            p.parent()
                .and_then(Path::file_name)
                .is_some_and(|n| n == "bin")
        })
        .map(|(p, _)| (p, p.parent().map_or(0, |d| d.components().count())))
        .collect();
    // Prefer only the shallowest `bin/` (the bundle's top-level bin — never a
    // nested `lib/.../bin`, e.g. a bundled node helper).
    if let Some(min) = in_bin.iter().map(|(_, depth)| *depth).min() {
        return in_bin
            .iter()
            .filter(|(_, depth)| *depth == min)
            .map(|(p, _)| (*p).clone())
            .collect();
    }
    // Flat tarballs: every executable regular file.
    files
        .iter()
        .filter(|(_, exec)| *exec)
        .map(|(p, _)| p.clone())
        .collect()
}

/// Extract each staged tarball and install its binaries into `bin_dir` (mode
/// `0755`), layout-aware via [`pick_binaries`]. Returns the installed binary
/// names. This is the single install step the duplicate acquisition paths
/// (install.sh, docker, setup module 08, deploy remediate) converge on.
pub fn install_staged(staged: &[PathBuf], bin_dir: &Path) -> Result<Vec<String>> {
    std::fs::create_dir_all(bin_dir)
        .with_context(|| format!("creating bin dir {}", bin_dir.display()))?;
    let mut installed = Vec::new();
    for tarball in staged {
        let name = tarball
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("pkg");
        let extract = tarball.with_file_name(format!(".extract-{name}"));
        let _ = std::fs::remove_dir_all(&extract);
        std::fs::create_dir_all(&extract)?;

        let status = Command::new("tar")
            .arg("-xzf")
            .arg(tarball)
            .arg("-C")
            .arg(&extract)
            .status()
            .with_context(|| format!("spawning tar for {}", tarball.display()))?;
        if !status.success() {
            bail!("failed to extract {}", tarball.display());
        }

        let mut files = Vec::new();
        walk_files(&extract, &mut files)?;
        let pairs: Vec<(PathBuf, bool)> = files
            .iter()
            .map(|p| (p.clone(), is_executable(p)))
            .collect();

        for src in pick_binaries(&pairs) {
            let bin = src
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let dest = bin_dir.join(&bin);
            std::fs::copy(&src, &dest)
                .with_context(|| format!("installing {bin} -> {}", dest.display()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perm = std::fs::metadata(&dest)?.permissions();
                perm.set_mode(0o755);
                std::fs::set_permissions(&dest, perm)?;
            }
            installed.push(bin);
        }
        let _ = std::fs::remove_dir_all(&extract);
    }
    Ok(installed)
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
            checksums_url: format!("{url}.checksums.txt"),
        }
    }

    /// Build the fake file table for one artifact whose checksums.txt entry
    /// matches (with a decoy line, to exercise per-filename lookup).
    fn good_files(url: &str, filename: &str, content: &[u8]) -> HashMap<String, Vec<u8>> {
        let mut files = HashMap::new();
        files.insert(url.to_string(), content.to_vec());
        files.insert(
            format!("{url}.checksums.txt"),
            format!(
                "{}  decoy-other.tar.gz\n{}  {filename}\n",
                sha256_hex(b"decoy"),
                sha256_hex(content)
            )
            .into_bytes(),
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
    fn checksum_for_picks_matching_line_and_rejects_missing() {
        let body = "aaaa  other.tar.gz\nbbbb  target.tar.gz\n";
        assert_eq!(checksum_for(body, "target.tar.gz").unwrap(), "bbbb");
        assert!(checksum_for(body, "absent.tar.gz").is_err());
    }

    #[test]
    fn checksum_for_strips_binary_marker_and_lowercases() {
        let body = "ABCD *target.tar.gz\n";
        assert_eq!(checksum_for(body, "target.tar.gz").unwrap(), "abcd");
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
        // Both the artifact and its release checksums.txt were fetched.
        assert_eq!(
            *fetcher.calls.borrow(),
            vec![url.to_string(), format!("{url}.checksums.txt")]
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
            format!("{url}.checksums.txt"),
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

    #[test]
    fn pick_binaries_prefers_bin_dir() {
        let files = vec![
            (PathBuf::from("/x/oqto-v1/bin/oqto"), true),
            (PathBuf::from("/x/oqto-v1/bin/oqto-runner"), true),
            (PathBuf::from("/x/oqto-v1/lib/thing.so"), false),
            (PathBuf::from("/x/oqto-v1/README.md"), false),
        ];
        assert_eq!(
            pick_binaries(&files),
            vec![
                PathBuf::from("/x/oqto-v1/bin/oqto"),
                PathBuf::from("/x/oqto-v1/bin/oqto-runner"),
            ]
        );
    }

    #[test]
    fn pick_binaries_ignores_nested_bin() {
        // a nested lib/.../bin (e.g. a bundled node helper) must NOT be picked.
        let files = vec![
            (PathBuf::from("/x/app/bin/app"), true),
            (PathBuf::from("/x/app/lib/sub/bin/helper.js"), true),
        ];
        assert_eq!(pick_binaries(&files), vec![PathBuf::from("/x/app/bin/app")]);
    }

    #[test]
    fn pick_binaries_falls_back_to_executables() {
        let files = vec![
            (PathBuf::from("/x/mmry"), true),
            (PathBuf::from("/x/mmry-mcp"), true),
            (PathBuf::from("/x/LICENSE"), false),
            (PathBuf::from("/x/README.md"), false),
        ];
        assert_eq!(
            pick_binaries(&files),
            vec![PathBuf::from("/x/mmry"), PathBuf::from("/x/mmry-mcp")]
        );
    }

    // Note: install_staged()'s IO (tar extract + copy + chmod) is verified
    // end-to-end on a real host (the cargo-test sandbox lacks a usable tar);
    // its selection logic is covered by pick_binaries_* above.
}
