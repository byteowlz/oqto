//! Manifest-driven artifact acquisition (ADR-0018 / vemr.9).
//!
//! Consumes the [`crate::deps::ArtifactRef`] plan and fetches + checksum-verifies
//! each artifact into a staging dir. Fetching is abstracted behind [`Fetcher`] so
//! the orchestration is unit-testable without network; the real impl shells out
//! to `curl` like the scripts it replaces. This is the single acquisition path
//! the four duplicate implementations converge on.

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::Read;
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

/// Lowercase hex sha256 of fixture bytes; production archives hash in chunks.
#[cfg(test)]
fn sha256_hex(bytes: &[u8]) -> String {
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
    // A local checksum file is not a signature, but it must unambiguously
    // identify this particular archive. Bound the metadata read separately
    // from the archive, whose digest is streamed below.
    let mut checksum_bytes = Vec::new();
    std::fs::File::open(checksum_file)
        .with_context(|| format!("Failed opening checksum {}", checksum_file.display()))?
        .take(8193)
        .read_to_end(&mut checksum_bytes)?;
    if checksum_bytes.len() > 8192 {
        bail!("checksum file is too large for one artifact");
    }
    let contents = std::str::from_utf8(&checksum_bytes).context("checksum file is not UTF-8")?;
    let mut lines = contents.lines();
    let line = lines.next().context("Checksum file missing hash")?;
    if lines.next().is_some() {
        bail!("checksum file must contain exactly one artifact");
    }
    let mut tokens = line.split_whitespace();
    let expected = tokens.next().context("Checksum file missing hash")?;
    let recorded = tokens
        .next()
        .context("Checksum file missing artifact filename")?;
    if tokens.next().is_some()
        || expected.len() != 64
        || !expected.bytes().all(|byte| byte.is_ascii_hexdigit())
        || Path::new(recorded.trim_start_matches('*')).file_name() != artifact.file_name()
    {
        bail!("checksum file does not uniquely identify this artifact");
    }
    let expected = expected.to_ascii_lowercase();

    let file = std::fs::File::open(artifact)
        .with_context(|| format!("Failed opening artifact {}", artifact.display()))?;
    let actual = sha256_reader(file)
        .with_context(|| format!("Failed hashing artifact {}", artifact.display()))?;

    if actual != expected {
        bail!(
            "Checksum mismatch for {} (expected {expected}, got {actual})",
            artifact.display()
        );
    }
    Ok(())
}

/// Keep artifact verification memory bounded even for large archives. Hash
/// matching proves byte integrity, not the publisher's identity.
fn sha256_reader(mut source: impl Read) -> Result<String> {
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// Extract the lowercase sha256 recorded for `filename` from a combined
/// `checksums.txt` body (`<sha256>  <filename>` lines, optional `*` binary
/// marker). Errors if no line matches.
pub fn checksum_for(checksums: &str, filename: &str) -> Result<String> {
    let mut found = None;
    for line in checksums.lines() {
        let mut parts = line.split_whitespace();
        let (Some(hash), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if name.trim_start_matches('*') == filename {
            if found.is_some() {
                bail!("checksums.txt has duplicate entries for {filename}");
            }
            found = Some(hash.to_ascii_lowercase());
        }
    }
    found.with_context(|| format!("checksums.txt has no entry for {filename}"))
}

/// Verify `artifact` against the sha256 recorded for `filename` in a combined
/// `checksums.txt` file — the format byteowlz/oqto releases actually publish
/// (one file per release, not a per-artifact `.sha256` sibling).
pub fn verify_against_checksums(
    artifact: &Path,
    checksums_file: &Path,
    filename: &str,
) -> Result<()> {
    let mut checksum_bytes = Vec::new();
    std::fs::File::open(checksums_file)
        .with_context(|| format!("Failed opening checksums {}", checksums_file.display()))?
        .take(1_048_577)
        .read_to_end(&mut checksum_bytes)?;
    if checksum_bytes.len() > 1_048_576 {
        bail!("combined checksums file is too large");
    }
    let checksums = std::str::from_utf8(&checksum_bytes).context("checksums file is not UTF-8")?;
    let expected = checksum_for(checksums, filename)
        .with_context(|| format!("in {}", checksums_file.display()))?;

    let file = std::fs::File::open(artifact)
        .with_context(|| format!("Failed opening artifact {}", artifact.display()))?;
    let actual = sha256_reader(file)
        .with_context(|| format!("Failed hashing artifact {}", artifact.display()))?;

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
    let in_bin: Vec<(&PathBuf, usize, bool)> = files
        .iter()
        .filter(|(p, _)| {
            p.parent()
                .and_then(Path::file_name)
                .is_some_and(|n| n == "bin")
        })
        .map(|(p, executable)| {
            (
                p,
                p.parent().map_or(0, |d| d.components().count()),
                *executable,
            )
        })
        .collect();
    // Prefer only the shallowest `bin/` (the bundle's top-level bin — never a
    // nested `lib/.../bin`, e.g. a bundled node helper).
    if let Some(min) = in_bin.iter().map(|(_, depth, _)| *depth).min() {
        return in_bin
            .iter()
            .filter(|(_, depth, executable)| *depth == min && *executable)
            .map(|(p, _, _)| (*p).clone())
            .collect();
    }
    // Flat tarballs: every executable regular file.
    files
        .iter()
        .filter(|(_, exec)| *exec)
        .map(|(p, _)| p.clone())
        .collect()
}

/// A checksummed archive paired with its explicitly declared binary names.
/// An archive cannot claim a platform executable or another tool's entrypoint.
pub struct StagedTool<'a> {
    pub name: &'a str,
    pub archive: &'a Path,
    pub binaries: &'a [String],
}

/// Reject path separators, dot components, and option-like names before using a
/// manifest name or archive entry as a destination path.
pub(crate) fn valid_bin_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && bytes.all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

/// Platform executables are activated only by `oqto-setup install`, never by
/// dependency acquisition. Check these even with a caller-supplied allowlist.
pub(crate) fn platform_bin(name: &str) -> bool {
    name.starts_with("oqto") || name == "pi-bridge"
}

/// Resolve a prior Oqto tool release without trusting an arbitrary symlink.
fn current_tool_release(root: &Path) -> Result<Option<PathBuf>> {
    let current = root.join("current");
    match std::fs::symlink_metadata(&current) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("reading {}", current.display())),
        Ok(meta) if meta.file_type().is_symlink() => {
            let release = std::fs::read_link(&current)?;
            if release.parent() != Some(root.join("releases").as_path())
                || !std::fs::symlink_metadata(&release)?.file_type().is_dir()
            {
                bail!(
                    "unrecognized Oqto tool release pointer: {}",
                    current.display()
                );
            }
            Ok(Some(release))
        }
        Ok(_) => bail!("unmanaged Oqto tool release pointer: {}", current.display()),
    }
}

/// Install an entire verified set through one versioned directory and a single
/// `current` pointer switch. Existing files/symlinks are never adopted. Only
/// entrypoints already pointing to our managed release may be upgraded.
/// A subsequent run rechecks identity rather than overwriting unrelated tools.
pub fn install_staged(staged: &[StagedTool<'_>], bin_dir: &Path) -> Result<Vec<String>> {
    if staged.is_empty() {
        return Ok(Vec::new());
    }
    let mut extracted = Vec::with_capacity(staged.len());
    let mut selected = Vec::new();
    let mut names = BTreeSet::new();
    for tool in staged {
        if !valid_bin_name(tool.name) || tool.binaries.is_empty() {
            bail!("missing or unsafe binary ownership for {}", tool.name);
        }
        let declared: BTreeSet<&str> = tool.binaries.iter().map(String::as_str).collect();
        if declared.len() != tool.binaries.len()
            || declared
                .iter()
                .any(|n| !valid_bin_name(n) || platform_bin(n))
        {
            bail!("duplicate or unsafe binary ownership for {}", tool.name);
        }
        let parent = tool
            .archive
            .parent()
            .context("staged tool archive has no parent")?;
        let extract = tempfile::Builder::new()
            .prefix(".oqto-acq-")
            .tempdir_in(parent)?;
        crate::archive::extract_tool_bundle(tool.archive, extract.path())
            .with_context(|| format!("rejecting unsafe tool archive {}", tool.archive.display()))?;
        let mut files = Vec::new();
        walk_files(extract.path(), &mut files)?;
        let pairs: Vec<_> = files
            .iter()
            .map(|p| (p.clone(), is_executable(p)))
            .collect();
        let binaries = pick_binaries(&pairs);
        if binaries.is_empty() {
            bail!(
                "staged tool archive contains no executable binaries: {}",
                tool.name
            );
        }
        for src in binaries {
            let bin = src
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if !declared.contains(bin) || !names.insert(bin.to_string()) {
                bail!("undeclared or duplicate tool binary {bin} in {}", tool.name);
            }
            selected.push((bin.to_string(), src));
        }
        if !names.contains(tool.name) {
            bail!("staged tool archive omits primary binary {}", tool.name);
        }
        extracted.push(extract);
    }

    match std::fs::symlink_metadata(bin_dir) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => std::fs::create_dir_all(bin_dir)?,
        Err(err) => return Err(err).with_context(|| format!("reading {}", bin_dir.display())),
        Ok(meta) if meta.file_type().is_dir() => (),
        Ok(_) => bail!(
            "tool binary directory must be a real directory: {}",
            bin_dir.display()
        ),
    }
    let root = bin_dir.join(".oqto-tools");
    match std::fs::symlink_metadata(&root) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            // A first install must not even create state in a directory that
            // already has an unmanaged executable at one of our destinations.
            for bin in &names {
                let dest = bin_dir.join(bin);
                match std::fs::symlink_metadata(&dest) {
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
                    Err(err) => {
                        return Err(err).with_context(|| format!("reading {}", dest.display()));
                    }
                    Ok(_) => bail!("refusing unmanaged tool entrypoint: {}", dest.display()),
                }
            }
            std::fs::create_dir(&root)?;
        }
        Err(err) => return Err(err).with_context(|| format!("reading {}", root.display())),
        Ok(meta) if meta.file_type().is_dir() => (),
        Ok(_) => bail!("unmanaged tool release directory: {}", root.display()),
    }
    let releases = root.join("releases");
    match std::fs::symlink_metadata(&releases) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => std::fs::create_dir(&releases)?,
        Err(err) => return Err(err).with_context(|| format!("reading {}", releases.display())),
        Ok(meta) if meta.file_type().is_dir() => (),
        Ok(_) => bail!("unmanaged tool release store: {}", releases.display()),
    }
    use std::os::unix::fs::OpenOptionsExt;
    let lock_path = root.join(".lock");
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&lock_path)?;
    if !lock.metadata()?.file_type().is_file() {
        bail!("unsafe tool release lock: {}", lock_path.display());
    }
    lock.lock().context("locking tool release state")?;

    let previous = current_tool_release(&root)?;
    let mut previous_names = BTreeSet::new();
    if let Some(ref prior) = previous {
        for entry in std::fs::read_dir(prior)? {
            let entry = entry?;
            let bin = entry.file_name().to_string_lossy().into_owned();
            if !valid_bin_name(&bin) || !entry.file_type()?.is_file() {
                bail!("unsafe managed tool release entry: {bin}");
            }
            previous_names.insert(bin);
        }
    }
    let managed: BTreeSet<String> = previous_names.union(&names).cloned().collect();
    for bin in &managed {
        let dest = bin_dir.join(bin);
        let expected = root.join("current").join(bin);
        match std::fs::symlink_metadata(&dest) {
            Err(err)
                if err.kind() == std::io::ErrorKind::NotFound && !previous_names.contains(bin) => {}
            Err(err) => return Err(err).with_context(|| format!("reading {}", dest.display())),
            Ok(meta)
                if previous_names.contains(bin)
                    && meta.file_type().is_symlink()
                    && std::fs::read_link(&dest)? == expected => {}
            Ok(_) => bail!("refusing unmanaged tool entrypoint: {}", dest.display()),
        }
    }
    let release = tempfile::Builder::new()
        .prefix(".oqto-acq-")
        .tempdir_in(&releases)?;
    if let Some(ref prior) = previous {
        for bin in &previous_names {
            std::fs::copy(prior.join(bin), release.path().join(bin))?;
        }
    }
    for (bin, src) in &selected {
        let dst = release.path().join(bin);
        std::fs::copy(src, &dst).with_context(|| format!("staging tool binary {bin}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dst, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    // Only newly introduced names need links. Old names already resolve via
    // `current`; the final pointer switch upgrades them all together.
    let mut added = Vec::new();
    let result: Result<()> = (|| {
        for bin in names.difference(&previous_names) {
            let dest = bin_dir.join(bin);
            let target = root.join("current").join(bin);
            std::os::unix::fs::symlink(&target, &dest)
                .with_context(|| format!("creating tool entrypoint {}", dest.display()))?;
            added.push(dest);
        }
        let final_release = release.keep();
        let next = tempfile::Builder::new()
            .prefix(".oqto-pointer-")
            .tempdir_in(&root)?;
        let pending = next.path().join("current");
        std::os::unix::fs::symlink(&final_release, &pending)?;
        std::fs::rename(&pending, root.join("current")).context("switching active tool release")?;
        Ok(())
    })();
    if let Err(err) = result {
        for link in added {
            if let Err(rollback) = std::fs::remove_file(&link) {
                return Err(err).context(format!(
                    "tool rollback ALSO failed for {}: {rollback}",
                    link.display()
                ));
            }
        }
        return Err(err);
    }
    Ok(names.into_iter().collect())
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
    fn release_hashing_never_requests_more_than_a_bounded_chunk() -> Result<()> {
        struct BoundedReader(std::io::Cursor<Vec<u8>>);
        impl Read for BoundedReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if buffer.len() > 8192 {
                    return Err(std::io::Error::other("unbounded archive read"));
                }
                self.0.read(buffer)
            }
        }
        let bytes = vec![b'x'; 8192 * 3 + 1];
        assert_eq!(
            sha256_reader(BoundedReader(std::io::Cursor::new(bytes.clone())))?,
            sha256_hex(&bytes)
        );
        Ok(())
    }

    #[test]
    fn single_artifact_checksum_rejects_combined_or_oversized_metadata() -> Result<()> {
        let root = tempfile::tempdir()?;
        let artifact = root.path().join("release.tar.gz");
        std::fs::write(&artifact, b"fixture")?;
        let checksum = root.path().join("release.sha256");
        let line = format!("{}  release.tar.gz\n", sha256_hex(b"fixture"));
        std::fs::write(&checksum, format!("{line}{line}"))?;
        assert!(verify_sha256(&artifact, &checksum).is_err());
        std::fs::write(&checksum, format!("{line}{}", "x".repeat(8193)))?;
        assert!(verify_sha256(&artifact, &checksum).is_err());
        std::fs::write(&checksum, line)?;
        verify_sha256(&artifact, &checksum)
    }

    #[test]
    fn checksum_for_picks_matching_line_and_rejects_missing() {
        let body = "aaaa  other.tar.gz\nbbbb  target.tar.gz\n";
        assert_eq!(checksum_for(body, "target.tar.gz").unwrap(), "bbbb");
        assert!(checksum_for(body, "absent.tar.gz").is_err());
    }

    #[test]
    fn checksum_for_rejects_duplicate_release_entries() {
        let body = "aaaa  target.tar.gz\nbbbb  target.tar.gz\n";
        assert!(checksum_for(body, "target.tar.gz").is_err());
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

    #[test]
    fn staged_tool_archive_installs_only_executable_top_level_bin() -> Result<()> {
        let root = tempfile::tempdir()?;
        let tarball = root.path().join("tool.tar.gz");
        let encoded = flate2::write::GzEncoder::new(
            std::fs::File::create(&tarball)?,
            flate2::Compression::default(),
        );
        let mut builder = tar::Builder::new(encoded);
        for (name, data, mode) in [
            ("pkg/bin/tool", &b"tool"[..], 0o755),
            ("pkg/bin/README", &b"docs"[..], 0o644),
            ("pkg/lib/sub/bin/helper", &b"helper"[..], 0o755),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(data.len() as u64);
            header.set_mode(mode);
            header.set_cksum();
            builder.append_data(&mut header, name, data)?;
        }
        builder.into_inner()?.finish()?;
        let bin_dir = root.path().join("installed-bin");
        let allowed = vec!["tool".to_owned()];
        let installed = install_staged(
            &[StagedTool {
                name: "tool",
                archive: &tarball,
                binaries: &allowed,
            }],
            &bin_dir,
        )?;
        assert_eq!(installed, vec!["tool"]);
        assert_eq!(std::fs::read(bin_dir.join("tool"))?, b"tool");
        assert!(!bin_dir.join("README").exists());
        assert!(!bin_dir.join("helper").exists());
        assert!(
            std::fs::read_dir(root.path())?
                .filter_map(std::result::Result::ok)
                .all(|entry| !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".oqto-acq-"))
        );
        Ok(())
    }

    fn tool_archive(path: &Path, names: &[(&str, &[u8])]) -> Result<()> {
        let encoded = flate2::write::GzEncoder::new(
            std::fs::File::create(path)?,
            flate2::Compression::default(),
        );
        let mut builder = tar::Builder::new(encoded);
        for (name, bytes) in names {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, format!("pkg/bin/{name}"), *bytes)?;
        }
        builder.into_inner()?.finish()?;
        Ok(())
    }

    #[test]
    fn staged_tools_refuse_unmanaged_and_platform_entrypoints_before_mutation() -> Result<()> {
        let root = tempfile::tempdir()?;
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin)?;
        std::fs::write(bin.join("tool"), b"user-owned")?;
        std::fs::write(bin.join(".tool.new"), b"unrelated temporary file")?;
        let archive = root.path().join("tool.tar.gz");
        tool_archive(&archive, &[("tool", b"replacement")])?;
        let allowed = vec!["tool".to_owned()];
        assert!(
            install_staged(
                &[StagedTool {
                    name: "tool",
                    archive: &archive,
                    binaries: &allowed
                }],
                &bin
            )
            .is_err()
        );
        assert_eq!(std::fs::read(bin.join("tool"))?, b"user-owned");
        assert_eq!(
            std::fs::read(bin.join(".tool.new"))?,
            b"unrelated temporary file"
        );
        assert!(!bin.join(".oqto-tools").exists());

        let malicious = root.path().join("malicious.tar.gz");
        tool_archive(&malicious, &[("oqtoctl", b"malicious")])?;
        std::fs::write(bin.join("oqtoctl"), b"platform-owned")?;
        assert!(
            install_staged(
                &[StagedTool {
                    name: "tool",
                    archive: &malicious,
                    binaries: &allowed
                }],
                &bin
            )
            .is_err()
        );
        assert_eq!(std::fs::read(bin.join("oqtoctl"))?, b"platform-owned");
        Ok(())
    }

    #[test]
    fn later_archive_failure_preserves_preexisting_tools_and_leaves_no_new_entries() -> Result<()> {
        let root = tempfile::tempdir()?;
        let bin = root.path().join("bin");
        let first = root.path().join("first.tar.gz");
        tool_archive(&first, &[("tool", b"first")])?;
        let corrupt = root.path().join("corrupt.tar.gz");
        std::fs::write(&corrupt, b"not a gzip")?;
        let allowed = vec!["tool".to_owned()];
        assert!(
            install_staged(
                &[
                    StagedTool {
                        name: "tool",
                        archive: &first,
                        binaries: &allowed
                    },
                    StagedTool {
                        name: "tool",
                        archive: &corrupt,
                        binaries: &allowed
                    },
                ],
                &bin
            )
            .is_err()
        );
        assert!(!bin.join("tool").exists());
        assert!(!bin.join(".tool.new").exists());
        Ok(())
    }

    #[test]
    fn staged_tool_refuses_unmanaged_symlink_and_release_store() -> Result<()> {
        let root = tempfile::tempdir()?;
        let bin = root.path().join("bin");
        let outside = root.path().join("outside");
        std::fs::create_dir(&bin)?;
        std::fs::create_dir(&outside)?;
        std::fs::write(outside.join("file"), b"preserve")?;
        let archive = root.path().join("tool.tar.gz");
        tool_archive(&archive, &[("tool", b"safe")])?;
        let allowed = vec!["tool".to_owned()];
        let input = [StagedTool {
            name: "tool",
            archive: &archive,
            binaries: &allowed,
        }];
        std::os::unix::fs::symlink(outside.join("file"), bin.join("tool"))?;
        assert!(install_staged(&input, &bin).is_err());
        assert_eq!(std::fs::read(outside.join("file"))?, b"preserve");
        std::fs::remove_file(bin.join("tool"))?;
        std::os::unix::fs::symlink(&outside, bin.join(".oqto-tools"))?;
        assert!(install_staged(&input, &bin).is_err());
        assert_eq!(std::fs::read_dir(&outside)?.count(), 1);
        assert!(!bin.join("tool").exists());
        Ok(())
    }

    #[test]
    fn multi_archive_upgrade_switches_once_and_failed_next_upgrade_preserves_prior_set()
    -> Result<()> {
        let root = tempfile::tempdir()?;
        let bin = root.path().join("bin");
        let first = root.path().join("tool-v1.tar.gz");
        let second = root.path().join("other-v1.tar.gz");
        tool_archive(&first, &[("tool", b"old tool")])?;
        tool_archive(&second, &[("other", b"old other")])?;
        let tool = vec!["tool".to_owned()];
        let other = vec!["other".to_owned()];
        let old = [
            StagedTool {
                name: "tool",
                archive: &first,
                binaries: &tool,
            },
            StagedTool {
                name: "other",
                archive: &second,
                binaries: &other,
            },
        ];
        assert_eq!(install_staged(&old, &bin)?, vec!["other", "tool"]);
        assert_eq!(std::fs::read(bin.join("tool"))?, b"old tool");
        assert_eq!(std::fs::read(bin.join("other"))?, b"old other");
        let current = bin.join(".oqto-tools/current");
        let previous = std::fs::read_link(&current)?;
        let entrypoint = std::fs::read_link(bin.join("tool"))?;

        let newer = root.path().join("tool-v2.tar.gz");
        tool_archive(&newer, &[("tool", b"new tool")])?;
        let corrupt = root.path().join("corrupt.tar.gz");
        std::fs::write(&corrupt, b"not a gzip")?;
        let failed = [
            StagedTool {
                name: "tool",
                archive: &newer,
                binaries: &tool,
            },
            StagedTool {
                name: "other",
                archive: &corrupt,
                binaries: &other,
            },
        ];
        assert!(install_staged(&failed, &bin).is_err());
        assert_eq!(std::fs::read_link(&current)?, previous);
        assert_eq!(std::fs::read(bin.join("tool"))?, b"old tool");
        assert_eq!(std::fs::read(bin.join("other"))?, b"old other");
        assert_eq!(std::fs::read_link(bin.join("tool"))?, entrypoint);

        let other_new = root.path().join("other-v2.tar.gz");
        tool_archive(&other_new, &[("other", b"new other")])?;
        let upgrade = [
            StagedTool {
                name: "tool",
                archive: &newer,
                binaries: &tool,
            },
            StagedTool {
                name: "other",
                archive: &other_new,
                binaries: &other,
            },
        ];
        assert_eq!(install_staged(&upgrade, &bin)?, vec!["other", "tool"]);
        assert_ne!(std::fs::read_link(&current)?, previous);
        assert_eq!(std::fs::read(bin.join("tool"))?, b"new tool");
        assert_eq!(std::fs::read(bin.join("other"))?, b"new other");
        assert_eq!(std::fs::read_link(bin.join("tool"))?, entrypoint);
        assert!(!bin.join(".tool.new").exists());
        Ok(())
    }

    #[test]
    fn staged_tool_archive_rejects_symlink_to_host_file() -> Result<()> {
        let root = tempfile::tempdir()?;
        let outside = root.path().join("outside-secret");
        std::fs::write(&outside, b"do not install host-owned files")?;
        let tarball = root.path().join("tool.tar.gz");
        let encoded = flate2::write::GzEncoder::new(
            std::fs::File::create(&tarball)?,
            flate2::Compression::default(),
        );
        let mut builder = tar::Builder::new(encoded);
        let mut binary = tar::Header::new_gnu();
        binary.set_entry_type(tar::EntryType::Regular);
        binary.set_size(5);
        binary.set_mode(0o755);
        binary.set_cksum();
        builder.append_data(&mut binary, "bin/tool", &b"hello"[..])?;
        let mut link = tar::Header::new_gnu();
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_size(0);
        link.set_mode(0o755);
        link.set_link_name("../../outside-secret")?;
        link.set_cksum();
        builder.append_data(&mut link, "bin/escape", std::io::empty())?;
        builder.into_inner()?.finish()?;
        let bin_dir = root.path().join("installed-bin");
        let allowed = vec!["tool".to_owned()];
        assert!(
            install_staged(
                &[StagedTool {
                    name: "tool",
                    archive: &tarball,
                    binaries: &allowed
                }],
                &bin_dir
            )
            .is_err()
        );
        assert_eq!(std::fs::read(outside)?, b"do not install host-owned files");
        assert!(!bin_dir.join("escape").exists());
        Ok(())
    }

    #[test]
    fn pick_binaries_does_not_promote_nonexecutable_bin_files() {
        let files = vec![
            (PathBuf::from("/x/app/bin/app"), true),
            (PathBuf::from("/x/app/bin/README"), false),
        ];
        assert_eq!(pick_binaries(&files), vec![PathBuf::from("/x/app/bin/app")]);
    }
}
