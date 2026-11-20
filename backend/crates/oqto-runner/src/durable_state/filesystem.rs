//! Persistent ownership identity, never a mount/device number.
use anyhow::Result;
use std::fs::File;

#[cfg(target_os = "macos")]
pub(super) fn identity(file: &File) -> Result<String> {
    let identity = oqto_personal::persistent_filesystem(file)?;
    anyhow::ensure!(
        identity.starts_with("macos-volume:"),
        "stable volume UUID unavailable"
    );
    Ok(identity)
}

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
pub(super) fn identity(file: &File) -> Result<String> {
    linux::identity(file)
}

#[cfg(not(any(
    target_os = "macos",
    all(target_os = "linux", target_pointer_width = "64")
)))]
pub(super) fn identity(_file: &File) -> Result<String> {
    anyhow::bail!("stable Durable volume identity is unsupported on this platform")
}

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
mod linux {
    use super::*;
    use anyhow::{Context, ensure};
    use std::os::fd::AsRawFd;

    // Linux UAPI linux/btrfs.h, BTRFS_IOC_FS_INFO and GET_SUBVOL_INFO.
    #[repr(C)]
    struct Volume {
        max_id: u64,
        num_devices: u64,
        fsid: [u8; 16],
        nodesize: u32,
        sectorsize: u32,
        clone_alignment: u32,
        csum_type: u16,
        csum_size: u16,
        flags: u64,
        generation: u64,
        metadata_uuid: [u8; 16],
        reserved: [u8; 944],
    }
    #[repr(C)]
    struct Timestamp {
        sec: u64,
        nsec: u32,
    }
    #[repr(C)]
    struct Subvolume {
        treeid: u64,
        name: [u8; 256],
        parent_id: u64,
        dirid: u64,
        generation: u64,
        flags: u64,
        uuid: [u8; 16],
        parent_uuid: [u8; 16],
        received_uuid: [u8; 16],
        transactions: [u64; 4],
        timestamps: [Timestamp; 4],
        reserved: [u64; 8],
    }
    fn qualified_kind(kind: u64) -> Result<()> {
        ensure!(
            kind == 0x9123_683e,
            "stable Durable Linux identity currently requires Btrfs; no device-number fallback"
        );
        Ok(())
    }
    fn label(volume: [u8; 16], subvolume: [u8; 16], treeid: u64) -> Result<String> {
        ensure!(volume != [0; 16], "Btrfs volume UUID unavailable");
        let scope = if subvolume != [0; 16] {
            uuid::Uuid::from_bytes(subvolume).to_string()
        } else {
            // The immutable top-level FS tree has the UAPI object id5, no UUID.
            ensure!(treeid == 5, "Btrfs subvolume UUID unavailable");
            "top-level".into()
        };
        Ok(format!(
            "linux-btrfs:{}:{scope}",
            uuid::Uuid::from_bytes(volume)
        ))
    }
    pub(super) fn identity(file: &File) -> Result<String> {
        qualified_kind(rustix::fs::fstatfs(file)?.f_type as u64)?;
        // SAFETY: these C UAPI structures contain only integers and byte arrays;
        // all-zero initialization is valid and keeps input/reserved fields zero.
        let mut volume: Volume = unsafe { std::mem::zeroed() };
        // SAFETY: live fd, platform-encoded read ioctl, correctly sized repr(C)
        // output; native header probe and layout regression verify the ABI.
        let result = unsafe {
            libc::ioctl(
                file.as_raw_fd(),
                libc::_IOR::<Volume>(0x94, 31),
                &mut volume,
            )
        };
        if result < 0 {
            return Err(std::io::Error::last_os_error()).context("read stable Btrfs volume UUID");
        }
        // SAFETY: integer/byte-array C UAPI POD; zero is valid for all fields.
        let mut subvolume: Subvolume = unsafe { std::mem::zeroed() };
        // SAFETY: live fd and correctly sized/aligned initialized C output.
        // GET_SUBVOL_INFO may return a positive success value; only -1 is error.
        let result = unsafe {
            libc::ioctl(
                file.as_raw_fd(),
                libc::_IOR::<Subvolume>(0x94, 60),
                &mut subvolume,
            )
        };
        if result < 0 {
            return Err(std::io::Error::last_os_error())
                .context("read stable Btrfs subvolume UUID");
        }
        label(volume.fsid, subvolume.uuid, subvolume.treeid)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[tokio::test]
        async fn unsupported_filesystem_refuses_before_creating_the_requested_root() -> Result<()> {
            let temp = tempfile::tempdir_in("/dev/shm")?;
            let file = File::open(temp.path())?;
            ensure!(
                rustix::fs::fstatfs(&file)?.f_type as u64 != 0x9123_683e,
                "negative fixture needs a non-Btrfs filesystem"
            );
            assert!(identity(&file).is_err());
            let root = temp.path().join("not-created");
            assert!(
                crate::durable_state::DurableState::open(&root)
                    .await
                    .is_err()
            );
            assert!(
                !root.exists(),
                "unsupported placement created state before capability admission"
            );
            Ok(())
        }
        #[test]
        fn native_uapi_layout_matches_kernel_header_probe() {
            assert_eq!(std::mem::size_of::<Volume>(), 1024);
            assert_eq!(std::mem::size_of::<Subvolume>(), 504);
            assert_eq!(std::mem::offset_of!(Subvolume, uuid), 296);
            assert_eq!(libc::_IOR::<Volume>(0x94, 31) as u32, 0x8400_941f);
            assert_eq!(libc::_IOR::<Subvolume>(0x94, 60) as u32, 0x81f8_943c);
        }
        #[test]
        fn volume_and_subvolume_are_persistent_identity_not_mount_or_tree_numbers() -> Result<()> {
            let a = label([1; 16], [2; 16], 257)?;
            assert_eq!(a, label([1; 16], [2; 16], 999)?);
            assert_ne!(a, label([3; 16], [2; 16], 257)?);
            assert_ne!(a, label([1; 16], [3; 16], 257)?);
            assert!(label([0; 16], [2; 16], 257).is_err());
            assert!(label([1; 16], [0; 16], 257).is_err());
            assert!(label([1; 16], [0; 16], 5)?.ends_with(":top-level"));
            assert!(qualified_kind(0x9123_683e).is_ok());
            for kind in [0x0102_1994, 0x794c_7630, 0xef53, 0x5846_5342, 0] {
                assert!(qualified_kind(kind).is_err());
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_storage_identity_never_uses_the_legacy_device_fallback() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let file = File::open(temp.path())?;
        let value = identity(&file)?;
        #[cfg(target_os = "macos")]
        assert!(value.starts_with("macos-volume:"));
        #[cfg(target_os = "linux")]
        assert!(value.starts_with("linux-btrfs:"));
        assert!(!value.starts_with("unix-device:"));
        assert_eq!(value, identity(&file)?);
        Ok(())
    }
}
