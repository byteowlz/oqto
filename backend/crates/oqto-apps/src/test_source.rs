//! In-memory [`AppFileSource`] used to exercise discovery and publication
//! without touching a real filesystem or an authorization boundary.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::source::{AppDirEntry, AppFileSource, AppFileStat, AppSourceError};

#[derive(Debug, Clone, PartialEq, Eq)]
enum MemoryEntry {
    Dir { symlink: bool },
    File { bytes: Vec<u8>, symlink: bool },
}

/// Work-directory-relative in-memory tree.
#[derive(Debug, Default, Clone)]
pub(crate) struct MemorySource {
    entries: BTreeMap<PathBuf, MemoryEntry>,
}

impl MemorySource {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn file(mut self, path: &str, bytes: impl Into<Vec<u8>>) -> Self {
        self.insert_file(path, bytes.into(), false);
        self
    }

    pub(crate) fn symlink_file(mut self, path: &str) -> Self {
        self.insert_file(path, Vec::new(), true);
        self
    }

    pub(crate) fn dir(mut self, path: &str) -> Self {
        self.insert_dirs(Path::new(path));
        self
    }

    /// Replace an existing file's bytes, mirroring an agent editing source
    /// between publications.
    pub(crate) fn with_edited_file(mut self, path: &str, bytes: impl Into<Vec<u8>>) -> Self {
        self.insert_file(path, bytes.into(), false);
        self
    }

    fn insert_file(&mut self, path: &str, bytes: Vec<u8>, symlink: bool) {
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent() {
            self.insert_dirs(parent);
        }
        self.entries
            .insert(path, MemoryEntry::File { bytes, symlink });
    }

    fn insert_dirs(&mut self, path: &Path) {
        let mut current = PathBuf::new();
        for component in path.components() {
            current.push(component);
            if current.as_os_str().is_empty() {
                continue;
            }
            self.entries
                .entry(current.clone())
                .or_insert(MemoryEntry::Dir { symlink: false });
        }
    }

    fn stat_of(entry: &MemoryEntry) -> AppFileStat {
        match entry {
            MemoryEntry::Dir { symlink } => AppFileStat {
                exists: true,
                is_file: false,
                is_dir: true,
                is_symlink: *symlink,
                size: 0,
                modified_at: 0,
            },
            MemoryEntry::File { bytes, symlink } => AppFileStat {
                exists: true,
                is_file: true,
                is_dir: false,
                is_symlink: *symlink,
                size: bytes.len() as u64,
                modified_at: 0,
            },
        }
    }
}

#[async_trait]
impl AppFileSource for MemorySource {
    async fn list_directory(
        &self,
        relative_path: &Path,
    ) -> Result<Option<Vec<AppDirEntry>>, AppSourceError> {
        if !matches!(
            self.entries.get(relative_path),
            Some(MemoryEntry::Dir { .. })
        ) {
            return Ok(None);
        }
        let mut listing = Vec::new();
        for (path, entry) in &self.entries {
            if path.parent() != Some(relative_path) {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let stat = Self::stat_of(entry);
            listing.push(AppDirEntry {
                name: name.to_owned(),
                is_file: stat.is_file,
                is_dir: stat.is_dir,
                is_symlink: stat.is_symlink,
                size: stat.size,
                modified_at: stat.modified_at,
            });
        }
        Ok(Some(listing))
    }

    async fn stat(&self, relative_path: &Path) -> Result<Option<AppFileStat>, AppSourceError> {
        Ok(self.entries.get(relative_path).map(Self::stat_of))
    }

    async fn read_file(
        &self,
        relative_path: &Path,
        max_bytes: u64,
    ) -> Result<Option<Vec<u8>>, AppSourceError> {
        match self.entries.get(relative_path) {
            Some(MemoryEntry::File { bytes, .. }) => {
                if bytes.len() as u64 > max_bytes {
                    return Err(AppSourceError::new("file exceeds the requested byte limit"));
                }
                Ok(Some(bytes.clone()))
            }
            _ => Ok(None),
        }
    }
}
