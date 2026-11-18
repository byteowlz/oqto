//! Ephemeral discovery acceleration, never identity authority. Every reuse still
//! checks no-follow metadata, owner, directory identity and the selected roots.
//! A scan cursor owns at most two directory descriptors and never discards
//! completed work just because its wall-clock or directory budget is exhausted.
use super::*;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

type Entries = std::iter::Peekable<std::fs::ReadDir>;

#[derive(Default)]
pub(crate) struct HeaderScan {
    root: Option<Entries>,
    current: Option<(DirectoryIdentity, Entries)>,
    directories: HashSet<PathBuf>,
    files: HashSet<PathBuf>,
    directory_count: usize,
    file_count: usize,
    pub(crate) headers: Vec<NativeHeader>,
    complete: bool,
}
impl HeaderScan {
    /// Cooperative budget: finish the current bounded per-file operation, then
    /// yield with the cursor intact. At least one entry advances on every call.
    /// No timeout drops a blocking task or restarts at the tree's first entry.
    pub(crate) fn advance(
        &mut self,
        source: &PersonalSource,
        directory_budget: usize,
        time_budget: Duration,
    ) -> Result<bool> {
        ensure!(
            directory_budget > 0,
            "Native scan requires a positive budget"
        );
        source.validate()?;
        if self.complete {
            return Ok(true);
        }
        let started = Instant::now();
        let mut completed = 0;
        let mut progressed = false;
        let mut cache = source
            .inventory
            .lock()
            .map_err(|_| anyhow::anyhow!("Native cache poisoned"))?;
        let root = match &mut self.root {
            Some(root) => root,
            slot @ None => slot.insert(std::fs::read_dir(source.session_root())?.peekable()),
        };
        if let Some((identity, _)) = &self.current {
            ensure!(
                cache.directory(&identity.path)? == *identity,
                "Native directory replaced during scan"
            );
        }
        loop {
            // Finalize directories before yielding, including the final chunk.
            if let Some((identity, entries)) = &mut self.current
                && entries.peek().is_none()
            {
                ensure!(
                    cache.directory(&identity.path)? == *identity,
                    "Native directory replaced during scan"
                );
                self.current = None;
                completed += 1;
            }
            if self.current.is_none() && root.peek().is_none() {
                source.validate()?;
                cache.retain(&self.files, &self.directories);
                self.complete = true;
                return Ok(true);
            }
            if completed >= directory_budget || (progressed && started.elapsed() >= time_budget) {
                source.validate()?;
                return Ok(false);
            }
            if let Some((_, entries)) = &mut self.current {
                if let Some(entry) = entries.next() {
                    ensure!(
                        self.file_count < MAX_HEADERS,
                        "Native header discovery bound exceeded"
                    );
                    self.file_count += 1;
                    progressed = true;
                    let entry = entry?;
                    let path = entry.path();
                    self.files.insert(path.clone());
                    if entry.file_type()?.is_file()
                        && path.extension().is_some_and(|ext| ext == "jsonl")
                        && let Ok(header) = cache.header(source, &path)
                    {
                        self.headers.push(header);
                    }
                }
            } else if let Some(directory) = root.next() {
                ensure!(
                    self.directory_count < MAX_DIRECTORIES,
                    "Native directory discovery bound exceeded"
                );
                self.directory_count += 1;
                progressed = true;
                let directory = directory?;
                let path = directory.path();
                if directory.file_type()?.is_dir()
                    && let Ok(identity) = cache.directory(&path)
                {
                    self.directories.insert(path.clone());
                    self.current = Some((identity, std::fs::read_dir(path)?.peekable()));
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileStamp {
    size: u64,
    mtime: (i64, i64),
    dev: u64,
    ino: u64,
}
impl FileStamp {
    fn read(meta: &std::fs::Metadata) -> Self {
        Self {
            size: meta.len(),
            mtime: (meta.mtime(), meta.mtime_nsec()),
            dev: meta.dev(),
            ino: meta.ino(),
        }
    }
}
#[derive(Debug, Default)]
pub(super) struct HeaderCache {
    files: HashMap<PathBuf, (FileStamp, NativeHeader)>,
    directories: HashMap<PathBuf, DirectoryIdentity>,
}
impl HeaderCache {
    #[cfg(test)]
    pub(super) fn files_contains_for_test(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }
    pub(super) fn directory(&mut self, path: &Path) -> Result<DirectoryIdentity> {
        let meta = std::fs::symlink_metadata(path)?;
        ensure!(
            meta.is_dir() && meta.uid() == rustix::process::geteuid().as_raw(),
            "Native directory owner denied"
        );
        if let Some(identity) = self.directories.get(path)
            && (identity.dev, identity.ino) == (meta.dev(), meta.ino())
        {
            return Ok(identity.clone());
        }
        // Directory replacement invalidates all children even if a child happens
        // to retain the same inode (e.g. moved into the replacement directory).
        self.files.retain(|file, _| file.parent() != Some(path));
        let identity = DirectoryIdentity::read(path.to_owned())?;
        self.directories.insert(path.to_owned(), identity.clone());
        Ok(identity)
    }
    pub(super) fn header(&mut self, source: &PersonalSource, path: &Path) -> Result<NativeHeader> {
        let meta = std::fs::symlink_metadata(path)?;
        ensure!(
            meta.is_file() && meta.uid() == rustix::process::geteuid().as_raw(),
            "Native file owner denied"
        );
        let stamp = FileStamp::read(&meta);
        if let Some((cached, header)) = self.files.get(path)
            && cached == &stamp
        {
            // cwd can have been replaced by a symlink without touching JSONL. A
            // deleted cwd stays admissible on the same terms as a fresh read.
            ensure!(
                super::canonical_or_vanished(&header.workspace)?,
                "Native cwd replaced"
            );
            return Ok(header.clone());
        }
        self.files.remove(path);
        let header = source.read_header(path, false)?;
        ensure!(
            stamp == FileStamp::read(&std::fs::symlink_metadata(path)?)
                && (header.dev, header.ino) == (stamp.dev, stamp.ino),
            "Native header changed during discovery"
        );
        self.files.insert(path.to_owned(), (stamp, header.clone()));
        Ok(header)
    }
    fn retain(&mut self, files: &HashSet<PathBuf>, directories: &HashSet<PathBuf>) {
        self.files.retain(|path, _| files.contains(path));
        self.directories
            .retain(|path, _| directories.contains(path));
    }
}
