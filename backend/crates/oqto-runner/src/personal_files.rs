//! Personal Files effects, invoked only by the Runner after device admission.
//! All resolution uses the selected, inode-validated directory descriptor.
use base64::Engine;
use cap_std::fs::{Dir, OpenOptions, OpenOptionsExt};
use oqto_personal::{AuthorizedAction, actions::ActionFailure};
use oqto_personal_wire::actions::{HostAction, MAX_FILE_CHUNK};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Component, Path},
};

const MAX_FILE: u64 = 8 * 1024 * 1024;
fn invalid<T>(_: T) -> ActionFailure {
    ActionFailure::Invalid
}
fn io_error(error: std::io::Error) -> ActionFailure {
    if error.kind() == std::io::ErrorKind::NotFound {
        ActionFailure::NotFound
    } else {
        ActionFailure::Denied
    }
}
fn relative(path: &str) -> Result<&Path, ActionFailure> {
    let value = Path::new(path);
    if path.len() > 4096
        || path.chars().any(char::is_control)
        || value
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return Err(ActionFailure::Invalid);
    }
    Ok(if path.is_empty() {
        Path::new(".")
    } else {
        value
    })
}
fn bytes(dir: &Dir, path: &Path) -> Result<Vec<u8>, ActionFailure> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let mut file = dir.open_with(path, &options).map_err(io_error)?;
    let meta = file.metadata().map_err(io_error)?;
    if !meta.is_file() || meta.len() > MAX_FILE {
        return Err(ActionFailure::Invalid);
    }
    let mut data = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_FILE + 1)
        .read_to_end(&mut data)
        .map_err(io_error)?;
    if data.len() as u64 > MAX_FILE {
        return Err(ActionFailure::Invalid);
    }
    Ok(data)
}
fn digest(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}
fn baseline(dir: &Dir, path: &Path, expected: Option<&str>) -> Result<(), ActionFailure> {
    match (bytes(dir, path), expected) {
        (Err(ActionFailure::NotFound), None) => Ok(()),
        (Ok(data), Some(hash)) if hash.len() == 64 && digest(&data) == hash => Ok(()),
        (Err(error), _) if !matches!(error, ActionFailure::NotFound) => Err(error),
        _ => Err(ActionFailure::StaleBaseline),
    }
}

/// The caller resolves the exact authorized Session, never a phone-supplied cwd.
pub(crate) fn session_context(
    admission: &AuthorizedAction,
    session_id: &str,
    cwd: &Path,
) -> Result<Value, ActionFailure> {
    let (root, path) = cwd_context(&admission.grants().files, cwd)?;
    Ok(
        json!({"session_id":session_id,"root":{"id":root.id(),"label":root.path().file_name().map(|s|s.to_string_lossy()).unwrap_or_default(),"writable":root.writable()},"path":path}),
    )
}

fn cwd_context<'a>(
    grants: &'a [oqto_personal::FileGrant],
    cwd: &Path,
) -> Result<(&'a oqto_personal::FileGrant, String), ActionFailure> {
    use cap_std::fs::MetadataExt as _;
    use std::os::unix::fs::MetadataExt;
    if !cwd.is_absolute() {
        return Err(ActionFailure::Invalid);
    }
    let cwd = cwd.canonicalize().map_err(io_error)?;
    let expected = cwd.metadata().map_err(io_error)?;
    let root = grants
        .iter()
        .filter(|r| cwd.starts_with(r.path()))
        .max_by_key(|r| r.path().components().count())
        .ok_or(ActionFailure::Denied)?;
    let path = cwd.strip_prefix(root.path()).map_err(invalid)?;
    let dir = Dir::from_std_file(root.open_root().map_err(invalid)?);
    let opened = dir
        .open_dir(relative(path.to_str().ok_or(ActionFailure::Invalid)?)?)
        .map_err(io_error)?;
    let actual = opened.dir_metadata().map_err(io_error)?;
    if actual.dev() != expected.dev() || actual.ino() != expected.ino() {
        return Err(ActionFailure::Denied);
    }
    Ok((root, path.to_str().ok_or(ActionFailure::Invalid)?.into()))
}

/// The embedding backend serializes Files operations. SHA preconditions detect
/// stale snapshots and competing API writes. POSIX cannot offer a global CAS
/// against unrelated native writers that do not participate in its lock.
pub(crate) fn execute(
    action: HostAction,
    admission: AuthorizedAction,
) -> Result<Value, ActionFailure> {
    let (root_id, path) = match &action {
        HostAction::FilesList { root_id, path }
        | HostAction::FileRead { root_id, path, .. }
        | HostAction::FileWrite { root_id, path, .. }
        | HostAction::FileRemove { root_id, path, .. }
        | HostAction::DirectoryCreate { root_id, path } => (root_id, path),
        _ => return Err(ActionFailure::Invalid),
    };
    let grant = admission
        .grants()
        .files
        .iter()
        .find(|grant| grant.id() == root_id)
        .ok_or(ActionFailure::Denied)?;
    let writing = matches!(
        action,
        HostAction::FileWrite { .. }
            | HostAction::FileRemove { .. }
            | HostAction::DirectoryCreate { .. }
    );
    if writing && !grant.writable() {
        return Err(ActionFailure::Denied);
    }
    let dir = Dir::from_std_file(grant.open_root().map_err(invalid)?);
    let path = relative(path)?;
    match &action {
        HostAction::FilesList { .. } => {
            let opened = dir.open_dir(path).map_err(io_error)?;
            let mut entries = Vec::new();
            for (seen, entry) in opened.entries().map_err(io_error)?.enumerate() {
                if seen >= 10000 {
                    return Err(ActionFailure::Invalid);
                }
                let entry = entry.map_err(io_error)?;
                let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                if name.chars().any(char::is_control) {
                    continue;
                }
                let kind = entry.file_type().map_err(io_error)?;
                // Do not follow symlinks to collect metadata, including list-only reads.
                let size = if kind.is_file() {
                    opened.symlink_metadata(&name).map_err(io_error)?.len()
                } else {
                    0
                };
                entries.push(json!({"name":name,"directory":kind.is_dir(),"symlink":kind.is_symlink(),"size":size}));
            }
            entries.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
            Ok(json!({"entries":entries}))
        }
        HostAction::FileRead { offset, limit, .. } => {
            if *limit == 0 || *limit as usize > MAX_FILE_CHUNK {
                return Err(ActionFailure::Invalid);
            }
            let data = bytes(&dir, path)?;
            let offset = usize::try_from(*offset).map_err(invalid)?;
            if offset > data.len() {
                return Err(ActionFailure::Invalid);
            }
            let end = offset.saturating_add(*limit as usize).min(data.len());
            Ok(
                json!({"data_base64":base64::engine::general_purpose::STANDARD.encode(&data[offset..end]),"sha256":digest(&data),"size":data.len(),"offset":offset,"eof":end==data.len()}),
            )
        }
        HostAction::DirectoryCreate { .. } => {
            dir.create_dir(path).map_err(io_error)?;
            Ok(json!({"created":true}))
        }
        HostAction::FileRemove {
            expected_sha256, ..
        } => {
            baseline(&dir, path, Some(expected_sha256))?;
            dir.remove_file(path).map_err(io_error)?;
            Ok(json!({"removed":true}))
        }
        HostAction::FileWrite {
            data_base64,
            expected_sha256,
            ..
        } => {
            let data = base64::engine::general_purpose::STANDARD
                .decode(data_base64)
                .map_err(invalid)?;
            if data.len() > MAX_FILE_CHUNK {
                return Err(ActionFailure::Invalid);
            }
            let hash = crate::checked_file::replace(&dir, path, &data, expected_sha256.as_deref())
                .map_err(|e| match e {
                    crate::checked_file::Failure::Invalid => ActionFailure::Invalid,
                    crate::checked_file::Failure::Conflict => ActionFailure::StaleBaseline,
                    crate::checked_file::Failure::Unknown => ActionFailure::OutcomeUnknown,
                    crate::checked_file::Failure::Io(e) => io_error(e),
                })?;
            Ok(json!({"written":data.len(),"sha256":hash}))
        }
        _ => Err(ActionFailure::Invalid),
    }
}

#[cfg(test)]
mod cwd_tests {
    use super::*;
    #[test]
    fn cwd_is_direct_scoped_most_specific_and_no_prefix_or_symlink_escape() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let cwd = root.join("project");
        std::fs::create_dir_all(&cwd).unwrap();
        let grants = vec![oqto_personal::FileGrant::select(&root, false).unwrap()];
        let (_, relative) = cwd_context(&grants, &cwd).unwrap();
        assert_eq!(relative, "project");
        let sibling = temp.path().join("root-other");
        std::fs::create_dir(&sibling).unwrap();
        assert!(cwd_context(&grants, &sibling).is_err());
        std::os::unix::fs::symlink(&sibling, root.join("escape")).unwrap();
        assert!(cwd_context(&grants, &root.join("escape")).is_err());
        let mut nested = grants.clone();
        nested.push(oqto_personal::FileGrant::select(&cwd, false).unwrap());
        let (selected, relative) = cwd_context(&nested, &cwd).unwrap();
        assert_eq!(selected.id(), nested[1].id());
        assert!(relative.is_empty());
        std::fs::rename(&root, temp.path().join("old-root")).unwrap();
        std::fs::create_dir_all(&cwd).unwrap();
        assert!(cwd_context(&grants, &cwd).is_err());
    }
}
