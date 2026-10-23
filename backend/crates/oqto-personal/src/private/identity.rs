//! Persistent file identity: macOS st_dev is a mount-instance ID, not a volume ID.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{fs::File, os::unix::fs::MetadataExt};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct StoreIdentity {
    pub purpose: String,
    pub filesystem: String,
    pub ino: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LegacyIdentity {
    pub purpose: String,
    pub dev: u64,
    pub ino: u64,
}

impl StoreIdentity {
    pub fn for_file(file: &File) -> Result<Self> {
        Ok(Self {
            purpose: "oqto.personal-store.v2".into(),
            filesystem: filesystem(file)?,
            ino: file.metadata()?.ino(),
        })
    }
    pub fn encode(&self) -> Result<String> {
        Ok(format!("{}\n", serde_json::to_string(self)?))
    }
}

/// Persistent filesystem identity for a file or directory: the volume UUID on
/// macOS (st_dev changes with each mount), the device number elsewhere.
#[cfg(not(target_os = "macos"))]
pub fn filesystem(file: &File) -> Result<String> {
    Ok(format!("unix-device:{}", file.metadata()?.dev()))
}

#[cfg(target_os = "macos")]
pub fn filesystem(file: &File) -> Result<String> {
    use std::os::fd::AsRawFd;
    #[repr(C)]
    struct VolumeUuid {
        length: u32,
        uuid: [u8; 16],
    }
    let mut request = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: 0,
        volattr: libc::ATTR_VOL_INFO | libc::ATTR_VOL_UUID,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let mut value = VolumeUuid {
        length: 0,
        uuid: [0; 16],
    };
    // SAFETY: live read-only descriptor; initialized attrlist and correctly sized
    // repr(C) output (length followed by the requested 16-byte volume UUID).
    let result = unsafe {
        libc::fgetattrlist(
            file.as_raw_fd(),
            (&mut request as *mut libc::attrlist).cast(),
            (&mut value as *mut VolumeUuid).cast(),
            std::mem::size_of::<VolumeUuid>(),
            0,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error()).context("reading stable macOS volume UUID");
    }
    ensure!(
        value.length as usize == std::mem::size_of::<VolumeUuid>()
            && value.uuid.iter().any(|b| *b != 0),
        "filesystem did not provide a stable volume UUID"
    );
    Ok(format!(
        "macos-volume:{}",
        uuid::Uuid::from_bytes(value.uuid)
    ))
}

pub(super) fn validate(contents: &str, expected: &StoreIdentity, dev: u64) -> Result<bool> {
    if let Ok(current) = serde_json::from_str::<StoreIdentity>(contents) {
        ensure!(
            current == *expected,
            "store ownership marker does not match persistent database identity"
        );
        return Ok(false);
    }
    let legacy: LegacyIdentity =
        serde_json::from_str(contents).context("unrecognized store ownership marker")?;
    ensure!(
        legacy.purpose == "oqto.personal-store.v1" && legacy.ino == expected.ino,
        "store ownership marker does not match database inode"
    );
    ensure!(
        legacy.dev == dev,
        "legacy store mount identity changed; explicit --repair-owner-marker SHA256 required (no database or grants changed)"
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persistent_marker_ignores_mount_instance_but_not_volume_or_inode() -> Result<()> {
        let expected = StoreIdentity {
            purpose: "oqto.personal-store.v2".into(),
            filesystem: "macos-volume:fixture".into(),
            ino: 42,
        };
        assert!(!validate(&expected.encode()?, &expected, 100)?);
        assert!(!validate(&expected.encode()?, &expected, 999)?);
        let other = StoreIdentity {
            filesystem: "macos-volume:foreign".into(),
            ..expected
        };
        assert!(
            validate(
                &other.encode()?,
                &StoreIdentity {
                    filesystem: "macos-volume:fixture".into(),
                    ..other
                },
                999
            )
            .is_err()
        );
        Ok(())
    }
}
