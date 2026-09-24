//! Capability-based runner file access. A path string is never authorization:
//! all operations are resolved relative to an already-open directory handle.
//! This module does not confine agent subprocesses or mounted filesystems.

use anyhow::{Context, Result, ensure};
use base64::Engine;
use cap_std::ambient_authority;
#[cfg(unix)]
use cap_std::fs::OpenOptionsExt;
use cap_std::fs::{Dir, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::protocol::{
    CreateDirectoryRequest, DeletePathRequest, DirEntry, DirectoryCreatedResponse,
    DirectoryListingResponse, ErrorCode, ErrorResponse, FileContentResponse, FileStatResponse,
    FileWrittenResponse, ListDirectoryRequest, PathDeletedResponse, ReadFileRequest, RunnerRequest,
    RunnerResponse, StatRequest, WriteFileRequest,
};

const MAX_READ_BYTES: u64 = 2 * 1024 * 1024;
const MAX_WRITE_BYTES: usize = 8 * 1024 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 10_000;
const MAX_CONCURRENT_FILE_OPERATIONS: usize = 8;

struct Root {
    absolute: PathBuf,
    handle: Dir,
}

/// Explicit roots opened once at startup. No path is resolved against the
/// runner's current working directory, nor against the backend host.
pub struct ScopedFiles {
    roots: Vec<Root>,
    slots: Arc<Semaphore>,
}

impl ScopedFiles {
    pub fn new(roots: &[PathBuf]) -> Result<Self> {
        ensure!(
            !roots.is_empty() && roots.len() <= 16,
            "1..=16 explicit runner roots required"
        );
        let mut opened = Vec::with_capacity(roots.len());
        for root in roots {
            ensure!(
                root.is_absolute() && !root.components().any(|c| matches!(c, Component::ParentDir)),
                "runner root must be an absolute path without parent traversal: {}",
                root.display()
            );
            let canonical = std::fs::canonicalize(root)
                .with_context(|| format!("canonicalizing runner root {}", root.display()))?;
            ensure!(
                root == &canonical,
                "runner root must be canonical (configure {} instead)",
                canonical.display()
            );
            let handle = Dir::open_ambient_dir(root, ambient_authority())
                .with_context(|| format!("opening runner root {}", root.display()))?;
            opened.push(Root {
                absolute: canonical,
                handle,
            });
        }
        Ok(Self {
            roots: opened,
            slots: Arc::new(Semaphore::new(MAX_CONCURRENT_FILE_OPERATIONS)),
        })
    }

    pub async fn acquire_slot(&self) -> Result<OwnedSemaphorePermit> {
        Ok(Arc::clone(&self.slots).acquire_owned().await?)
    }

    fn resolve<'a>(&'a self, path: &'a Path) -> Result<(&'a Dir, &'a Path)> {
        ensure!(
            path.is_absolute() && !path.components().any(|c| matches!(c, Component::ParentDir)),
            "absolute paths without parent traversal are required"
        );
        // Prefer the most specific grant, so overlapping roots have a stable
        // interpretation. cap-std confines symlinks beneath this handle.
        let root = self
            .roots
            .iter()
            .filter(|root| path.starts_with(&root.absolute))
            .max_by_key(|root| root.absolute.components().count())
            .ok_or_else(|| anyhow::anyhow!("path is outside configured runner roots"))?;
        let relative = path.strip_prefix(&root.absolute)?;
        Ok((
            &root.handle,
            if relative.as_os_str().is_empty() {
                Path::new(".")
            } else {
                relative
            },
        ))
    }

    fn read(&self, request: ReadFileRequest) -> Result<RunnerResponse> {
        let (root, relative) = self.resolve(&request.path)?;
        let limit = request.limit.unwrap_or(MAX_READ_BYTES);
        ensure!(
            limit <= MAX_READ_BYTES,
            "file read limit exceeds runner maximum"
        );
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY);
        let mut file = root.open_with(relative, &options)?;
        let meta = file.metadata()?;
        ensure!(meta.is_file(), "only regular files may be read");
        let offset = request.offset.unwrap_or(0);
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = Vec::new();
        file.take(limit).read_to_end(&mut bytes)?;
        let truncated = offset.saturating_add(bytes.len() as u64) < meta.len();
        Ok(RunnerResponse::FileContent(FileContentResponse {
            path: request.path,
            content_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            size: meta.len(),
            truncated,
        }))
    }

    fn write(&self, request: WriteFileRequest) -> Result<RunnerResponse> {
        let (root, relative) = self.resolve(&request.path)?;
        ensure!(
            request.content_base64.len() <= (MAX_WRITE_BYTES / 3 + 1) * 4,
            "file write exceeds runner maximum"
        );
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&request.content_base64)
            .context("invalid base64 file content")?;
        ensure!(
            bytes.len() <= MAX_WRITE_BYTES,
            "file write exceeds runner maximum"
        );
        if request.create_parents {
            let parent = relative.parent().context("file path has no parent")?;
            root.create_dir_all(parent)?;
        }
        let mut options = OpenOptions::new();
        options.write(true).create(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY);
        let mut file = root.open_with(relative, &options)?;
        ensure!(
            file.metadata()?.is_file(),
            "only regular files may be written"
        );
        file.set_len(0)?;
        file.write_all(&bytes)?;
        Ok(RunnerResponse::FileWritten(FileWrittenResponse {
            path: request.path,
            bytes_written: bytes.len() as u64,
        }))
    }

    fn list(&self, request: ListDirectoryRequest) -> Result<RunnerResponse> {
        let (root, relative) = self.resolve(&request.path)?;
        let mut entries = Vec::new();
        for entry in root.read_dir(relative)? {
            let entry = entry?;
            let filename = entry.file_name();
            let name = filename.to_string_lossy().to_string();
            if !request.include_hidden && name.starts_with('.') {
                continue;
            }
            ensure!(
                entries.len() < MAX_DIRECTORY_ENTRIES,
                "directory listing exceeds runner maximum"
            );
            // lstat, not stat: listing a link must not follow it (including
            // links that lead out of the granted root).
            let meta = root.symlink_metadata(relative.join(filename))?;
            entries.push(DirEntry {
                name,
                is_dir: meta.is_dir(),
                is_symlink: meta.file_type().is_symlink(),
                size: meta.len(),
                modified_at: timestamp_ms(meta.modified().ok()),
            });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(RunnerResponse::DirectoryListing(DirectoryListingResponse {
            path: request.path,
            entries,
        }))
    }

    fn stat(&self, request: StatRequest) -> Result<RunnerResponse> {
        let (root, relative) = self.resolve(&request.path)?;
        let meta = match root.symlink_metadata(relative) {
            Ok(meta) => meta,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(RunnerResponse::FileStat(FileStatResponse {
                    path: request.path,
                    exists: false,
                    is_file: false,
                    is_dir: false,
                    is_symlink: false,
                    size: 0,
                    modified_at: 0,
                    created_at: None,
                    mode: 0,
                }));
            }
            Err(err) => return Err(err.into()),
        };
        #[cfg(unix)]
        let mode = {
            use cap_std::fs::PermissionsExt;
            meta.permissions().mode()
        };
        #[cfg(not(unix))]
        let mode = 0;
        Ok(RunnerResponse::FileStat(FileStatResponse {
            path: request.path,
            exists: true,
            is_file: meta.is_file(),
            is_dir: meta.is_dir(),
            is_symlink: meta.file_type().is_symlink(),
            size: meta.len(),
            modified_at: timestamp_ms(meta.modified().ok()),
            created_at: meta.created().ok().map(|t| timestamp_ms(Some(t))),
            mode,
        }))
    }

    fn delete(&self, request: DeletePathRequest) -> Result<RunnerResponse> {
        let (root, relative) = self.resolve(&request.path)?;
        ensure!(relative != Path::new("."), "cannot delete runner root");
        let meta = root.symlink_metadata(relative)?;
        if meta.is_dir() {
            if request.recursive {
                root.remove_dir_all(relative)?;
            } else {
                root.remove_dir(relative)?;
            }
        } else {
            root.remove_file(relative)?;
        }
        Ok(RunnerResponse::PathDeleted(PathDeletedResponse {
            path: request.path,
        }))
    }

    fn mkdir(&self, request: CreateDirectoryRequest) -> Result<RunnerResponse> {
        let (root, relative) = self.resolve(&request.path)?;
        if request.create_parents {
            root.create_dir_all(relative)?;
        } else {
            root.create_dir(relative)?;
        }
        Ok(RunnerResponse::DirectoryCreated(DirectoryCreatedResponse {
            path: request.path,
        }))
    }

    /// Called on a blocking worker after the transport gate matched a Files
    /// variant; new request variants fail closed rather than bypassing scope.
    pub fn execute(&self, request: RunnerRequest) -> RunnerResponse {
        let result = match request {
            RunnerRequest::ReadFile(r) => self.read(r),
            RunnerRequest::WriteFile(r) => self.write(r),
            RunnerRequest::ListDirectory(r) => self.list(r),
            RunnerRequest::Stat(r) => self.stat(r),
            RunnerRequest::DeletePath(r) => self.delete(r),
            RunnerRequest::CreateDirectory(r) => self.mkdir(r),
            _ => Err(anyhow::anyhow!("request is not a scoped Files operation")),
        };
        match result {
            Ok(response) => response,
            Err(error) => {
                let code = if let Some(io) = error.downcast_ref::<std::io::Error>() {
                    match io.kind() {
                        std::io::ErrorKind::NotFound => ErrorCode::PathNotFound,
                        std::io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
                        _ => ErrorCode::IoError,
                    }
                } else {
                    ErrorCode::PermissionDenied
                };
                RunnerResponse::Error(ErrorResponse {
                    code,
                    message: format!("{error:#}"),
                })
            }
        }
    }
}

fn timestamp_ms(time: Option<cap_std::time::SystemTime>) -> i64 {
    time.and_then(|value| value.into_std().duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|value| i64::try_from(value.as_millis()).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_relative_paths_and_symlink_escape() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let base = std::fs::canonicalize(temp.path())?;
        let allowed = base.join("allowed");
        let other = base.join("other");
        std::fs::create_dir_all(&allowed)?;
        std::fs::create_dir_all(&other)?;
        std::fs::write(other.join("secret"), b"private")?;
        let files = ScopedFiles::new(std::slice::from_ref(&allowed))?;
        for candidate in [
            other.join("secret"),
            allowed.join("../other/secret"),
            PathBuf::from("secret"),
        ] {
            assert!(files.resolve(&candidate).is_err(), "{candidate:?}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&other, allowed.join("alias"))?;
            let listed = files.execute(RunnerRequest::ListDirectory(ListDirectoryRequest {
                path: allowed.clone(),
                include_hidden: true,
            }));
            let RunnerResponse::DirectoryListing(listed) = listed else {
                anyhow::bail!("symlink should be listed without following: {listed:?}")
            };
            assert!(
                listed
                    .entries
                    .iter()
                    .any(|entry| entry.name == "alias" && entry.is_symlink)
            );
            let result = files.execute(RunnerRequest::ReadFile(ReadFileRequest {
                path: allowed.join("alias/secret"),
                offset: None,
                limit: None,
            }));
            assert!(matches!(result, RunnerResponse::Error(_)), "{result:?}");
            for request in [
                RunnerRequest::WriteFile(WriteFileRequest {
                    path: allowed.join("alias/secret"),
                    content_base64: base64::engine::general_purpose::STANDARD.encode("corrupt"),
                    create_parents: false,
                }),
                RunnerRequest::ListDirectory(ListDirectoryRequest {
                    path: allowed.join("alias"),
                    include_hidden: true,
                }),
                RunnerRequest::CreateDirectory(CreateDirectoryRequest {
                    path: allowed.join("alias/new"),
                    create_parents: true,
                }),
                RunnerRequest::DeletePath(DeletePathRequest {
                    path: allowed.join("alias/secret"),
                    recursive: false,
                }),
            ] {
                let result = files.execute(request);
                assert!(matches!(result, RunnerResponse::Error(_)), "{result:?}");
            }
            assert_eq!(std::fs::read(other.join("secret"))?, b"private");
            assert!(!other.join("new").exists());
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn replacing_the_root_path_does_not_rebind_the_open_grant() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let base = std::fs::canonicalize(temp.path())?;
        let allowed = base.join("allowed");
        let moved = base.join("moved");
        let outside = base.join("outside");
        std::fs::create_dir(&allowed)?;
        std::fs::create_dir(&outside)?;
        std::fs::write(allowed.join("inside"), b"granted")?;
        std::fs::write(outside.join("secret"), b"ungranted")?;
        let files = ScopedFiles::new(std::slice::from_ref(&allowed))?;
        std::fs::rename(&allowed, &moved)?;
        std::os::unix::fs::symlink(&outside, &allowed)?;
        let denied = files.execute(RunnerRequest::ReadFile(ReadFileRequest {
            path: allowed.join("secret"),
            offset: None,
            limit: None,
        }));
        assert!(matches!(denied, RunnerResponse::Error(_)), "{denied:?}");
        let granted = files.execute(RunnerRequest::ReadFile(ReadFileRequest {
            path: allowed.join("inside"),
            offset: None,
            limit: None,
        }));
        assert!(
            matches!(granted, RunnerResponse::FileContent(_)),
            "{granted:?}"
        );
        Ok(())
    }

    #[test]
    fn explicit_filesystem_root_grants_access_outside_the_default_work_directory() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let files = ScopedFiles::new(&[PathBuf::from("/")])?;
        let response = files.execute(RunnerRequest::ListDirectory(ListDirectoryRequest {
            path: std::fs::canonicalize(temp.path())?,
            include_hidden: true,
        }));
        assert!(
            matches!(response, RunnerResponse::DirectoryListing(_)),
            "{response:?}"
        );
        Ok(())
    }

    #[test]
    fn scoped_file_writes_and_reads_stay_inside_root() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = std::fs::canonicalize(temp.path())?.join("root");
        std::fs::create_dir(&root)?;
        let files = ScopedFiles::new(std::slice::from_ref(&root))?;
        let path = root.join("nested/data.txt");
        let written = files.execute(RunnerRequest::WriteFile(WriteFileRequest {
            path: path.clone(),
            content_base64: base64::engine::general_purpose::STANDARD.encode("hello"),
            create_parents: true,
        }));
        assert!(
            matches!(written, RunnerResponse::FileWritten(_)),
            "{written:?}"
        );
        let read = files.execute(RunnerRequest::ReadFile(ReadFileRequest {
            path,
            offset: None,
            limit: Some(3),
        }));
        let RunnerResponse::FileContent(read) = read else {
            anyhow::bail!("expected scoped content: {read:?}")
        };
        assert_eq!(
            base64::engine::general_purpose::STANDARD.decode(read.content_base64)?,
            b"hel"
        );
        assert!(read.truncated);
        Ok(())
    }
}
