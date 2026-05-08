use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const OQTO_LOG_FILE_NAME: &str = "oqto-log.sqlite";

/// Resolve oqto-log db path in the owning Linux user's home directory.
///
/// Layout:
/// ~/.local/share/oqto/oqto-log/<workspace_hash>/oqto-log.sqlite
pub fn resolve_user_home_workspace_db_path(
    user_home: &Path,
    workspace_id: &str,
) -> Result<PathBuf> {
    let workspace_hash = id_hash(workspace_id);

    let dir = user_home
        .join(".local")
        .join("share")
        .join("oqto")
        .join("oqto-log")
        .join(workspace_hash);

    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating oqto-log directory: {}", dir.display()))?;

    Ok(dir.join(OQTO_LOG_FILE_NAME))
}

fn id_hash(id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(id.as_bytes());
    let digest = hasher.finalize();
    hex::encode(&digest[..12])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic_for_same_workspace_id() {
        assert_eq!(id_hash("workspace-abc"), id_hash("workspace-abc"));
    }

    #[test]
    fn workspace_path_is_in_user_home_oqto_log_dir() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = resolve_user_home_workspace_db_path(temp.path(), "workspace-123").expect("path");
        let s = path.display().to_string();
        assert!(s.contains("/.local/share/oqto/oqto-log/"));
        assert!(s.ends_with("/oqto-log.sqlite"));
    }
}
