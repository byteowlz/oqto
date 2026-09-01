use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use oqto_apps::{AppDirEntry, AppFileSource, AppFileStat, AppSourceError};

use crate::user_plane::UserPlane;

/// Adapter from an authorization-bound runner UserPlane to the host-neutral App
/// package source seam.
#[derive(Clone)]
pub struct UserPlaneAppSource {
    plane: Arc<dyn UserPlane>,
    work_directory_root: PathBuf,
}

impl std::fmt::Debug for UserPlaneAppSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UserPlaneAppSource")
            .field("work_directory_root", &"<authorized>")
            .finish()
    }
}

impl UserPlaneAppSource {
    #[must_use]
    pub fn new(plane: Arc<dyn UserPlane>, work_directory_root: PathBuf) -> Self {
        Self {
            plane,
            work_directory_root,
        }
    }

    fn resolve(&self, relative_path: &Path) -> Result<PathBuf, AppSourceError> {
        if relative_path.is_absolute()
            || relative_path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(AppSourceError::new(
                "App source path must be relative without dot or parent components",
            ));
        }
        Ok(self.work_directory_root.join(relative_path))
    }
}

#[async_trait]
impl AppFileSource for UserPlaneAppSource {
    async fn list_directory(
        &self,
        relative_path: &Path,
    ) -> Result<Option<Vec<AppDirEntry>>, AppSourceError> {
        let path = self.resolve(relative_path)?;
        let stat = self
            .plane
            .stat(&path)
            .await
            .map_err(|error| AppSourceError::new(format!("stat failed: {error:#}")))?;
        if !stat.exists {
            return Ok(None);
        }
        if !stat.is_dir || stat.is_symlink {
            return Err(AppSourceError::new(
                "requested App source directory is not a regular directory",
            ));
        }

        let entries = self
            .plane
            .list_directory(&path, true)
            .await
            .map_err(|error| AppSourceError::new(format!("directory listing failed: {error:#}")))?;
        Ok(Some(
            entries
                .into_iter()
                .map(|entry| AppDirEntry {
                    name: entry.name,
                    is_file: !entry.is_dir && !entry.is_symlink,
                    is_dir: entry.is_dir,
                    is_symlink: entry.is_symlink,
                    size: entry.size,
                    modified_at: entry.modified_at,
                })
                .collect(),
        ))
    }

    async fn stat(&self, relative_path: &Path) -> Result<Option<AppFileStat>, AppSourceError> {
        let path = self.resolve(relative_path)?;
        let stat = self
            .plane
            .stat(&path)
            .await
            .map_err(|error| AppSourceError::new(format!("stat failed: {error:#}")))?;
        if !stat.exists {
            return Ok(None);
        }
        Ok(Some(AppFileStat {
            exists: stat.exists,
            is_file: stat.is_file,
            is_dir: stat.is_dir,
            is_symlink: stat.is_symlink,
            size: stat.size,
            modified_at: stat.modified_at,
        }))
    }

    async fn read_file(
        &self,
        relative_path: &Path,
        max_bytes: u64,
    ) -> Result<Option<Vec<u8>>, AppSourceError> {
        let path = self.resolve(relative_path)?;
        let stat = self
            .plane
            .stat(&path)
            .await
            .map_err(|error| AppSourceError::new(format!("stat failed: {error:#}")))?;
        if !stat.exists {
            return Ok(None);
        }
        if !stat.is_file || stat.is_symlink {
            return Err(AppSourceError::new(
                "requested App source file is not a regular file",
            ));
        }
        if stat.size > max_bytes {
            return Err(AppSourceError::new(format!(
                "bounded read refused {} bytes over limit {max_bytes}",
                stat.size
            )));
        }
        let content = self
            .plane
            .read_file(&path, None, Some(max_bytes))
            .await
            .map_err(|error| AppSourceError::new(format!("file read failed: {error:#}")))?;
        if content.truncated || content.size > max_bytes {
            return Err(AppSourceError::new("bounded App source read was truncated"));
        }
        Ok(Some(content.content))
    }
}
