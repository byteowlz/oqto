//! Shared descriptor-confined, baseline-checked atomic replacement for both
//! personal and Server adapters. Native writers do not participate in a global CAS.
use cap_std::fs::{Dir, OpenOptions, OpenOptionsExt};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Component, Path},
};
#[derive(Debug)]
pub(crate) enum Failure {
    Invalid,
    Conflict,
    Io(std::io::Error),
    Unknown,
}
impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
fn bytes(dir: &Dir, path: &Path) -> Result<Vec<u8>, Failure> {
    // This is a single destination component beneath the already-open parent.
    // cap-std resolves final symlinks before applying custom O_NOFOLLOW, so use
    // a safe openat wrapper here rather than relying on that custom flag.
    let fd = rustix::fs::openat(
        dir,
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK,
        rustix::fs::Mode::empty(),
    )
    .map_err(|e| Failure::Io(e.into()))?;
    let mut file = std::fs::File::from(fd);
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() > 8 * 1024 * 1024 {
        return Err(Failure::Invalid);
    }
    let mut data = Vec::new();
    Read::by_ref(&mut file)
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut data)?;
    if data.len() > 8 * 1024 * 1024 {
        return Err(Failure::Invalid);
    }
    Ok(data)
}
fn hash(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}
fn baseline(dir: &Dir, path: &Path, expected: Option<&str>) -> Result<(), Failure> {
    match (bytes(dir, path), expected) {
        (Err(Failure::Io(e)), None) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        (Ok(data), Some(h)) if h.len() == 64 && hash(&data) == h => Ok(()),
        (Err(Failure::Io(e)), Some(_)) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(Failure::Conflict)
        }
        (Err(e), _) => Err(e),
        _ => Err(Failure::Conflict),
    }
}
pub(crate) fn replace(
    dir: &Dir,
    path: &Path,
    data: &[u8],
    expected: Option<&str>,
) -> Result<String, Failure> {
    if data.len() > 8 * 1024 * 1024
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(Failure::Invalid);
    }
    let name = path.file_name().ok_or(Failure::Invalid)?;
    let parent = dir.open_dir(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    let destination = Path::new(name);
    let mut sync_options = OpenOptions::new();
    sync_options
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW);
    let sync_parent = parent.open_with(".", &sync_options)?;
    baseline(&parent, destination, expected)?;
    let temp = format!(".oqto-write-{}", uuid::Uuid::new_v4());
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true).mode(0o600);
        let mut file = parent.open_with(&temp, &options)?;
        if expected.is_some() {
            file.set_permissions(parent.symlink_metadata(destination)?.permissions())?;
        }
        file.write_all(data)?;
        file.sync_all()?;
        baseline(&parent, destination, expected)?;
        if expected.is_none() {
            parent.hard_link(&temp, &parent, destination).map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    Failure::Conflict
                } else {
                    Failure::Io(e)
                }
            })?;
        } else {
            parent.rename(&temp, &parent, destination)?;
        }
        sync_parent.sync_all().map_err(|_| Failure::Unknown)?;
        Ok(hash(data))
    })();
    let _ = parent.remove_file(&temp);
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_and_symlink_are_refused_and_success_is_exact() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = Dir::open_ambient_dir(tmp.path(), cap_std::ambient_authority()).unwrap();
        std::fs::write(tmp.path().join("a"), b"authored []\r\n").unwrap();
        let old = hash(b"authored []\r\n");
        assert!(matches!(
            replace(&dir, Path::new("a"), b"new", Some(&hash(b"wrong"))),
            Err(Failure::Conflict)
        ));
        assert_eq!(
            std::fs::read(tmp.path().join("a")).unwrap(),
            b"authored []\r\n"
        );
        assert_eq!(
            replace(&dir, Path::new("a"), b"changed []\r\n", Some(&old)).unwrap(),
            hash(b"changed []\r\n")
        );
        std::os::unix::fs::symlink("a", tmp.path().join("link")).unwrap();
        assert!(
            replace(
                &dir,
                Path::new("link"),
                b"bad",
                Some(&hash(b"changed []\r\n"))
            )
            .is_err()
        );
        assert!(replace(&dir, Path::new("../escape"), b"bad", None).is_err());
    }
}
