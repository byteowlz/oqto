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
    let base = sessions_base_dir()?;
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
    find_in_all_directories(&base, &suffix)
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
pub async fn find_session_file_async(
    session_id: String,
    cwd: Option<PathBuf>,
) -> Option<PathBuf> {
    tokio::task::spawn_blocking(move || find_session_file(&session_id, cwd.as_deref()))
        .await
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cwd_to_safe_dirname() {
        assert_eq!(
            cwd_to_safe_dirname(Path::new("/home/user/projects/app")),
            "--home-user-projects-app--"
        );
        assert_eq!(
            cwd_to_safe_dirname(Path::new("/home/wismut/byteowlz/octo")),
            "--home-wismut-byteowlz-octo--"
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
