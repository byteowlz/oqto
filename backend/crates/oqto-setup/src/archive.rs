//! Shared bounded tar extraction for complete Oqto releases and separately
//! checksummed byteowlz tool bundles. Nothing here executes archive members.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

const MAX_ENTRIES: usize = 100_000;
const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub(crate) fn extract_release(artifact: &Path, dst: &Path, root: &str) -> Result<()> {
    extract_with_limits(artifact, dst, Some(root), MAX_ENTRIES, MAX_UNPACKED_BYTES)
}

pub(crate) fn extract_tool_bundle(artifact: &Path, dst: &Path) -> Result<()> {
    extract_with_limits(artifact, dst, None, MAX_ENTRIES, MAX_UNPACKED_BYTES)
}

pub(crate) fn extract_with_limits(
    artifact: &Path,
    dst: &Path,
    expected_root: Option<&str>,
    max_entries: usize,
    max_unpacked_bytes: u64,
) -> Result<()> {
    let source = File::open(artifact)
        .with_context(|| format!("Failed opening archive {}", artifact.display()))?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(source));
    let mut seen = HashSet::new();
    let mut total_bytes = 0_u64;
    for entry in archive.entries().context("Invalid archive")? {
        let mut entry = entry.context("Invalid archive entry")?;
        let path = entry.path().context("Invalid archive path")?.into_owned();
        let mut components = path.components().filter(|part| *part != Component::CurDir);
        if let Some(root) = expected_root
            && components.next() != Some(Component::Normal(root.as_ref()))
        {
            anyhow::bail!("Invalid archive: entry outside its declared root");
        }
        let mut relative = PathBuf::new();
        for component in components {
            match component {
                Component::Normal(name) => relative.push(name),
                _ => anyhow::bail!("Invalid archive: unsafe member path"),
            }
        }
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            anyhow::bail!("Invalid archive: links and special files are forbidden");
        }
        if relative.as_os_str().is_empty() && !kind.is_dir() {
            anyhow::bail!("Invalid archive: root must be a directory");
        }
        if !seen.insert(relative.clone()) || seen.len() > max_entries {
            anyhow::bail!("Invalid archive: duplicate or excessive members");
        }
        let size = entry.size();
        total_bytes = total_bytes
            .checked_add(size)
            .context("Archive size overflow")?;
        if total_bytes > max_unpacked_bytes {
            anyhow::bail!("Invalid archive: unpacked content exceeds limit");
        }
        let mode = entry.header().mode().context("Invalid archive file mode")?;
        if mode & 0o7000 != 0 {
            anyhow::bail!("Invalid archive: privileged file mode");
        }
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = dst.join(relative);
        if kind.is_dir() {
            if let Ok(existing) = fs::symlink_metadata(&target) {
                if !existing.file_type().is_dir() {
                    anyhow::bail!("Invalid archive: directory collides with file");
                }
            } else {
                fs::create_dir_all(&target)?;
            }
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            let copied = io::copy(&mut entry, &mut output)?;
            if copied != size {
                anyhow::bail!("Invalid archive: truncated file data");
            }
            fs::set_permissions(&target, fs::Permissions::from_mode(mode & 0o777))?;
        }
    }
    if seen.is_empty() {
        anyhow::bail!("Invalid archive: no members");
    }
    // Force the gzip CRC/trailer even though tar stops at zero-filled markers.
    let decoder = archive.into_inner();
    let padding = io::copy(&mut decoder.take(1_048_577), &mut io::sink())?;
    if padding > 1_048_576 {
        anyhow::bail!("Invalid archive: excessive trailing content");
    }
    Ok(())
}
