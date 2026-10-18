//! Pi session file discovery.
//!
//! Pi stores session files at:
//!   `~/.pi/agent/sessions/--{safe_cwd}--/{timestamp}_{session_id}.jsonl`
//!
//! Where `safe_cwd` is the working directory with `/` replaced by `-` and
//! leading `/` stripped. For example:
//!   `/home/user/projects/myapp` -> `home-user-projects-myapp`
//!
//! This module provides utilities to find session files by session ID,
//! enabling session resumption for externally-created Pi sessions.

use std::path::{Path, PathBuf};

use log::{debug, warn};

/// Default Pi sessions directory relative to user home.
const PI_SESSIONS_REL: &str = ".pi/agent/sessions";

/// Convert a working directory path to Pi's safe directory name.
///
/// Pi replaces `/` with `-` and wraps in `--..--`.
/// Example: `/home/user/project` -> `--home-user-project--`
fn cwd_to_safe_dirname(cwd: &Path) -> String {
    let path_str = cwd.to_string_lossy();
    // Strip leading slash and replace remaining slashes with dashes
    let safe = path_str
        .strip_prefix('/')
        .unwrap_or(&path_str)
        .replace('/', "-");
    format!("--{}--", safe)
}

/// Get the Pi sessions base directory.
///
/// Returns `~/.pi/agent/sessions` (respects $HOME).
fn sessions_base_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(PI_SESSIONS_REL))
}

/// Find a Pi session JSONL file by session ID.
///
/// Searches `~/.pi/agent/sessions/` for a file matching `*_{session_id}.jsonl`.
/// If `cwd` is provided, searches only the matching workspace directory first,
/// then falls back to searching all directories.
///
/// Returns the full path to the session file if found.
pub fn find_session_file(session_id: &str, cwd: Option<&Path>) -> Option<PathBuf> {
    find_session_file_in(&sessions_base_dir()?, session_id, cwd)
}

/// Find a Pi session JSONL file beneath an explicit sessions directory.
///
/// Only the host that owns the files can resolve them, so callers acting for
/// another machine must supply that machine's directory rather than relying on
/// this process's `$HOME`.
pub fn find_session_file_in(base: &Path, session_id: &str, cwd: Option<&Path>) -> Option<PathBuf> {
    if !base.exists() {
        debug!("Pi sessions directory not found: {:?}", base);
        return None;
    }

    let suffix = format!("_{}.jsonl", session_id);

    // If cwd is provided, search the workspace-specific directory first
    if let Some(cwd) = cwd {
        let dirname = cwd_to_safe_dirname(cwd);
        let workspace_dir = base.join(&dirname);
        if let Some(path) = find_in_directory(&workspace_dir, &suffix) {
            return Some(path);
        }
    }

    // Fall back: search all session directories
    find_in_all_directories(base, &suffix)
}

/// Strict adoption lookup: one bound native id in its designated Workspace,
/// never a cross-directory fallback or a first-of-many selection. The caller
/// still validates the native header/cwd, peer and current host grant.
pub fn find_unique_session_file_in(
    base: &Path,
    session_id: &str,
    cwd: &Path,
) -> std::io::Result<Option<PathBuf>> {
    if session_id.is_empty()
        || session_id.len() > 256
        || !session_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid native id",
        ));
    }
    let directory = base.join(cwd_to_safe_dirname(cwd));
    let metadata = match std::fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(std::io::Error::other("Unsafe native Workspace directory"));
    }
    let suffix = format!("_{session_id}.jsonl");
    let mut found = None;
    for (count, entry) in std::fs::read_dir(directory)?.enumerate() {
        if count >= 32768 {
            return Err(std::io::Error::other(
                "Native Workspace directory exceeds adoption bound",
            ));
        }
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.ends_with(&suffix))
        {
            if !entry.file_type()?.is_file() || found.is_some() {
                return Err(std::io::Error::other("Ambiguous or unsafe native source"));
            }
            found = Some(entry.path());
        }
    }
    Ok(found)
}

/// Search a specific directory for a session file with the given suffix.
fn find_in_directory(dir: &Path, suffix: &str) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            warn!("Failed to read Pi sessions directory {:?}: {}", dir, e);
            return None;
        }
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.ends_with(suffix) {
            let path = entry.path();
            debug!("Found Pi session file: {:?}", path);
            return Some(path);
        }
    }

    None
}

/// Search all subdirectories of the sessions base for a session file.
fn find_in_all_directories(base: &Path, suffix: &str) -> Option<PathBuf> {
    let entries = match std::fs::read_dir(base) {
        Ok(entries) => entries,
        Err(e) => {
            warn!("Failed to read Pi sessions base {:?}: {}", base, e);
            return None;
        }
    };

    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        if let Some(path) = find_in_directory(&entry.path(), suffix) {
            return Some(path);
        }
    }

    None
}

/// Async wrapper for find_session_file (runs blocking I/O on spawn_blocking).
pub async fn find_session_file_async(session_id: String, cwd: Option<PathBuf>) -> Option<PathBuf> {
    tokio::task::spawn_blocking(move || find_session_file(&session_id, cwd.as_deref()))
        .await
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_adoption_rejects_ambiguity_and_never_falls_back_to_other_workspace()
    -> std::io::Result<()> {
        // A private std-only fixture keeps this wire/helper crate dependency-light.
        let base = std::env::temp_dir().join(format!(
            "oqto-pi-unique-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(std::io::Error::other)?
                .as_nanos()
        ));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        std::fs::create_dir(&base)?;
        let _cleanup = Cleanup(base.clone());
        let workspace = Path::new("/synthetic/work");
        let other = base.join(cwd_to_safe_dirname(Path::new("/other/work")));
        std::fs::create_dir(&other)?;
        std::fs::write(other.join("first_native-a.jsonl"), b"{}\n")?;
        assert!(find_unique_session_file_in(&base, "native-a", workspace)?.is_none());
        let directory = base.join(cwd_to_safe_dirname(workspace));
        std::fs::create_dir(&directory)?;
        let first = directory.join("first_native-a.jsonl");
        std::fs::write(&first, b"{}\n")?;
        assert_eq!(
            find_unique_session_file_in(&base, "native-a", workspace)?,
            Some(first)
        );
        std::fs::write(directory.join("second_native-a.jsonl"), b"{}\n")?;
        assert!(find_unique_session_file_in(&base, "native-a", workspace).is_err());
        assert!(find_unique_session_file_in(&base, "../native-a", workspace).is_err());
        Ok(())
    }

    #[test]
    fn test_cwd_to_safe_dirname() {
        assert_eq!(
            cwd_to_safe_dirname(Path::new("/home/user/projects/app")),
            "--home-user-projects-app--"
        );
        assert_eq!(
            cwd_to_safe_dirname(Path::new("/home/wismut/byteowlz/oqto")),
            "--home-wismut-byteowlz-oqto--"
        );
        // Edge case: root
        assert_eq!(cwd_to_safe_dirname(Path::new("/")), "----");
    }

    #[test]
    fn test_find_session_file_nonexistent() {
        // Should return None for nonexistent session
        let result = find_session_file("nonexistent-session-id-12345", None);
        assert!(result.is_none());
    }
}
