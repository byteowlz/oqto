//! Direct user-plane implementation.
//!
//! Provides direct filesystem/database access for single-user and container modes.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;

use super::UserPlane;
use super::types::*;

/// Direct user-plane implementation using local filesystem access.
///
/// Used in:
/// - Single-user local mode (no isolation needed)
/// - Container mode (isolation provided by container)
#[derive(Debug, Clone)]
pub struct DirectUserPlane {
    /// Root directory for the user's workspace.
    workspace_root: std::path::PathBuf,
}

impl DirectUserPlane {
    /// Create a new direct user-plane with the given workspace root.
    pub fn new(workspace_root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
        }
    }

    /// Resolve and validate a path within the workspace.
    fn resolve_path(&self, path: &Path) -> Result<std::path::PathBuf> {
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace_root.join(path)
        };

        // Canonicalize to resolve symlinks and check it exists
        // For paths that don't exist yet, canonicalize the parent
        if resolved.exists() {
            let canonical = resolved
                .canonicalize()
                .with_context(|| format!("canonicalizing path {:?}", resolved))?;

            // Verify it's within workspace root
            let workspace_canonical = self
                .workspace_root
                .canonicalize()
                .unwrap_or_else(|_| self.workspace_root.clone());
            if !canonical.starts_with(&workspace_canonical) {
                anyhow::bail!("path {:?} is outside workspace root", path);
            }

            Ok(canonical)
        } else {
            // Path doesn't exist - check parent is valid
            if let Some(parent) = resolved.parent() {
                if parent.exists() {
                    let parent_canonical = parent.canonicalize()?;
                    let workspace_canonical = self
                        .workspace_root
                        .canonicalize()
                        .unwrap_or_else(|_| self.workspace_root.clone());
                    if !parent_canonical.starts_with(&workspace_canonical) {
                        anyhow::bail!("path {:?} is outside workspace root", path);
                    }
                }
            }
            Ok(resolved)
        }
    }
}

#[async_trait]
impl UserPlane for DirectUserPlane {
    async fn read_file(
        &self,
        path: &Path,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> Result<FileContent> {
        let resolved = self.resolve_path(path)?;

        let content = tokio::fs::read(&resolved)
            .await
            .with_context(|| format!("reading file {:?}", resolved))?;

        let size = content.len() as u64;

        let (data, truncated) = if let Some(limit) = limit {
            let offset = offset.unwrap_or(0) as usize;
            let end = (offset + limit as usize).min(content.len());
            let slice = &content[offset.min(content.len())..end];
            (slice.to_vec(), end < content.len())
        } else {
            (content, false)
        };

        Ok(FileContent {
            content: data,
            size,
            truncated,
        })
    }

    async fn write_file(&self, path: &Path, content: &[u8], create_parents: bool) -> Result<()> {
        let resolved = self.resolve_path(path)?;

        if create_parents {
            if let Some(parent) = resolved.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .with_context(|| format!("creating parent directories for {:?}", resolved))?;
            }
        }

        tokio::fs::write(&resolved, content)
            .await
            .with_context(|| format!("writing file {:?}", resolved))?;

        Ok(())
    }

    async fn list_directory(&self, path: &Path, include_hidden: bool) -> Result<Vec<DirEntry>> {
        let resolved = self.resolve_path(path)?;

        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(&resolved)
            .await
            .with_context(|| format!("reading directory {:?}", resolved))?;

        while let Some(entry) = dir.next_entry().await? {
            let name = entry.file_name().to_string_lossy().to_string();

            if !include_hidden && name.starts_with('.') {
                continue;
            }

            let metadata = entry.metadata().await?;

            let modified_at = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            entries.push(DirEntry {
                name,
                is_dir: metadata.is_dir(),
                is_symlink: metadata.is_symlink(),
                size: metadata.len(),
                modified_at,
            });
        }

        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    async fn stat(&self, path: &Path) -> Result<FileStat> {
        let resolved = self.resolve_path(path)?;

        match tokio::fs::metadata(&resolved).await {
            Ok(metadata) => {
                let modified_at = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);

                let created_at = metadata
                    .created()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64);

                #[cfg(unix)]
                let mode = {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode()
                };
                #[cfg(not(unix))]
                let mode = 0o644;

                Ok(FileStat {
                    exists: true,
                    is_file: metadata.is_file(),
                    is_dir: metadata.is_dir(),
                    is_symlink: metadata.is_symlink(),
                    size: metadata.len(),
                    modified_at,
                    created_at,
                    mode,
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(FileStat {
                exists: false,
                is_file: false,
                is_dir: false,
                is_symlink: false,
                size: 0,
                modified_at: 0,
                created_at: None,
                mode: 0,
            }),
            Err(e) => Err(e.into()),
        }
    }

    async fn delete_path(&self, path: &Path, recursive: bool) -> Result<()> {
        let resolved = self.resolve_path(path)?;

        let metadata = tokio::fs::metadata(&resolved)
            .await
            .with_context(|| format!("getting metadata for {:?}", resolved))?;

        if metadata.is_dir() {
            if recursive {
                tokio::fs::remove_dir_all(&resolved).await?;
            } else {
                tokio::fs::remove_dir(&resolved).await?;
            }
        } else {
            tokio::fs::remove_file(&resolved).await?;
        }

        Ok(())
    }

    async fn create_directory(&self, path: &Path, create_parents: bool) -> Result<()> {
        let resolved = self.resolve_path(path)?;

        if create_parents {
            tokio::fs::create_dir_all(&resolved).await?;
        } else {
            tokio::fs::create_dir(&resolved).await?;
        }

        Ok(())
    }

    // Session operations - these need to interact with a session database
    // For DirectUserPlane, we'd need a DB connection. For now, return empty.

    async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        // TODO: Connect to user's session database
        Ok(Vec::new())
    }

    async fn get_session(&self, _session_id: &str) -> Result<Option<SessionInfo>> {
        // TODO: Connect to user's session database
        Ok(None)
    }

    async fn start_session(&self, _request: StartSessionRequest) -> Result<StartSessionResponse> {
        // TODO: Implement session starting
        anyhow::bail!("Session management not implemented for DirectUserPlane")
    }

    async fn stop_session(&self, _session_id: &str) -> Result<()> {
        // TODO: Implement session stopping
        anyhow::bail!("Session management not implemented for DirectUserPlane")
    }

    // Main chat operations

    async fn list_main_chat_sessions(&self) -> Result<Vec<MainChatSessionInfo>> {
        // TODO: List Pi session files from ~/.pi/agent/sessions/
        Ok(Vec::new())
    }

    async fn get_main_chat_messages(
        &self,
        _session_id: &str,
        _limit: Option<usize>,
    ) -> Result<Vec<MainChatMessage>> {
        // TODO: Parse Pi session .jsonl file
        Ok(Vec::new())
    }

    // Memory operations

    async fn search_memories(
        &self,
        _query: &str,
        _limit: usize,
        _category: Option<&str>,
    ) -> Result<MemorySearchResults> {
        // TODO: Search mmry database
        Ok(MemorySearchResults {
            memories: Vec::new(),
            total: 0,
        })
    }

    async fn add_memory(
        &self,
        _content: &str,
        _category: Option<&str>,
        _importance: Option<u8>,
    ) -> Result<String> {
        // TODO: Add to mmry database
        anyhow::bail!("Memory operations not implemented for DirectUserPlane")
    }

    async fn delete_memory(&self, _memory_id: &str) -> Result<()> {
        // TODO: Delete from mmry database
        anyhow::bail!("Memory operations not implemented for DirectUserPlane")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_read_write_file() {
        let temp = tempdir().unwrap();
        let up = DirectUserPlane::new(temp.path());

        // Write a file
        let content = b"Hello, World!";
        up.write_file(Path::new("test.txt"), content, false)
            .await
            .unwrap();

        // Read it back
        let result = up
            .read_file(Path::new("test.txt"), None, None)
            .await
            .unwrap();
        assert_eq!(result.content, content);
        assert_eq!(result.size, content.len() as u64);
        assert!(!result.truncated);
    }

    #[tokio::test]
    async fn test_list_directory() {
        let temp = tempdir().unwrap();
        let up = DirectUserPlane::new(temp.path());

        // Create some files
        up.write_file(Path::new("a.txt"), b"a", false)
            .await
            .unwrap();
        up.write_file(Path::new("b.txt"), b"b", false)
            .await
            .unwrap();
        up.write_file(Path::new(".hidden"), b"h", false)
            .await
            .unwrap();

        // List without hidden
        let entries = up.list_directory(Path::new("."), false).await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "a.txt");
        assert_eq!(entries[1].name, "b.txt");

        // List with hidden
        let entries = up.list_directory(Path::new("."), true).await.unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[tokio::test]
    async fn test_stat() {
        let temp = tempdir().unwrap();
        let up = DirectUserPlane::new(temp.path());

        // Stat non-existent
        let stat = up.stat(Path::new("nonexistent")).await.unwrap();
        assert!(!stat.exists);

        // Create and stat
        up.write_file(Path::new("test.txt"), b"hello", false)
            .await
            .unwrap();
        let stat = up.stat(Path::new("test.txt")).await.unwrap();
        assert!(stat.exists);
        assert!(stat.is_file);
        assert!(!stat.is_dir);
        assert_eq!(stat.size, 5);
    }

    #[tokio::test]
    async fn test_create_directory() {
        let temp = tempdir().unwrap();
        let up = DirectUserPlane::new(temp.path());

        up.create_directory(Path::new("subdir"), false)
            .await
            .unwrap();

        let stat = up.stat(Path::new("subdir")).await.unwrap();
        assert!(stat.exists);
        assert!(stat.is_dir);
    }

    #[tokio::test]
    async fn test_delete_path() {
        let temp = tempdir().unwrap();
        let up = DirectUserPlane::new(temp.path());

        // Create and delete file
        up.write_file(Path::new("test.txt"), b"hello", false)
            .await
            .unwrap();
        up.delete_path(Path::new("test.txt"), false).await.unwrap();
        let stat = up.stat(Path::new("test.txt")).await.unwrap();
        assert!(!stat.exists);

        // Create and delete directory
        up.create_directory(Path::new("subdir"), false)
            .await
            .unwrap();
        up.write_file(Path::new("subdir/file.txt"), b"hello", false)
            .await
            .unwrap();
        up.delete_path(Path::new("subdir"), true).await.unwrap();
        let stat = up.stat(Path::new("subdir")).await.unwrap();
        assert!(!stat.exists);
    }
}
