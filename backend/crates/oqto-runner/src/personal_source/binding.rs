//! Private owner-declared source selection, frozen before Runner workers start.
//! This stores paths only; native JSONL remains read-only/Pi-owned and canonical
//! writes remain in the unchanged service HOME through its one ingest actor.
use super::PersonalSource;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub schema: u32,
    pub session_root: PathBuf,
    pub socket_root: PathBuf,
    pub workspaces: Vec<PathBuf>,
}
impl Binding {
    pub fn resolve(&self, home: &Path) -> Result<PersonalSource> {
        ensure!(self.schema == 1, "unsupported native-source binding schema");
        PersonalSource::new(
            home.to_owned(),
            self.session_root.clone(),
            self.socket_root.clone(),
            self.workspaces.clone(),
        )
    }
}
fn path(home: &Path) -> PathBuf {
    crate::pair_control::root(home).join("native-source.json")
}

pub fn load(home: &Path) -> Result<Option<PersonalSource>> {
    read(home)?.map(|value| value.resolve(home)).transpose()
}
/// Resolve one immutable source before worker creation; conflicting owner
/// declarations are an error, never an implicit source switch.
pub fn for_startup(
    home: &Path,
    explicit: Option<std::sync::Arc<PersonalSource>>,
) -> Result<Option<std::sync::Arc<PersonalSource>>> {
    let Some(stored) = load(home)? else {
        return Ok(explicit);
    };
    if let Some(explicit) = &explicit {
        ensure!(
            explicit.home == stored.home
                && explicit.session == stored.session
                && explicit.socket == stored.socket
                && explicit.workspaces == stored.workspaces,
            "conflicting explicit and persisted native source declarations"
        );
    }
    Ok(Some(std::sync::Arc::new(stored)))
}
fn read(home: &Path) -> Result<Option<Binding>> {
    let path = path(home);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        other => {
            other?;
        }
    }
    crate::pair_control::private_dir(path.parent().context("binding parent missing")?)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file()
            && metadata.uid() == rustix::process::geteuid().as_raw()
            && metadata.mode() & 0o777 == 0o600,
        "untrusted native-source binding"
    );
    let mut bytes = Vec::new();
    (&mut file).take(32769).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 32768, "native-source binding too large");
    Ok(Some(serde_json::from_slice(&bytes)?))
}

/// Create a new owner declaration; preserve any existing declaration verbatim.
/// Caller must have obtained explicit source/Workspace consent before invoking.
pub fn install(home: &Path, binding: &Binding) -> Result<()> {
    binding.resolve(home)?;
    if let Some(current) = read(home)? {
        ensure!(
            current == *binding,
            "native source already bound differently; preserving existing declaration"
        );
        return Ok(());
    }
    let path = path(home);
    let parent = path.parent().context("binding parent missing")?;
    crate::pair_control::private_dir(parent)?;
    let mut staged = tempfile::Builder::new()
        .prefix(".native-source-")
        .tempfile_in(parent)?;
    staged
        .as_file_mut()
        .write_all(&serde_json::to_vec(binding)?)?;
    staged.as_file_mut().sync_all()?;
    staged
        .persist_noclobber(&path)
        .map_err(|error| anyhow::anyhow!("native-source publication refused: {}", error.error))?;
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests;
