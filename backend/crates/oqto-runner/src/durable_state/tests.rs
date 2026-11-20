//! Disposable state only; no provider, installed runtime, user profile or brain.
use super::*;
use std::{
    fs,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
};
fn home(agent: &str) -> AgentHome {
    AgentHome {
        workspace_id: "fixture-workspace".into(),
        agent_id: agent.into(),
    }
}
fn chmod(path: &Path, mode: u32) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[tokio::test]
async fn private_bootstrap_reopens_stable_identity_without_runtime_or_brain() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("state");
    let state = DurableState::open(&root).await?;
    let lease = state.lease_store(&home("agent")).await?;
    let id = lease.record().store_id.clone();
    let data = lease.data_directory().to_owned();
    lease.revalidate().await?;
    let record = state
        .bind_session(&home("agent"), "public", "native", "work")
        .await?;
    assert_eq!(record.store_id, id);
    assert_eq!(
        fs::metadata(root.join("control/catalog.sqlite3"))?.mode() & 0o077,
        0
    );
    assert!(!data.starts_with(root.join("control")));
    drop(lease);
    state.close().await;
    let reopened = DurableState::open(&root).await?;
    assert_eq!(
        reopened.resolve(&home("agent"), "public").await?,
        Some(record)
    );
    let lease = reopened.lease_store(&home("agent")).await?;
    assert_eq!(lease.record().store_id, id);
    assert_eq!(lease.data_directory(), data);
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn same_store_has_one_owner_and_other_stores_remain_available() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("state");
    let state = DurableState::open(&root).await?;
    let first = state.lease_store(&home("a")).await?;
    let path = root
        .join("control/locks")
        .join(format!("{}.lock", first.record().store_id));
    let inode = fs::metadata(&path)?.ino();
    let second_state = DurableState::open(&root).await?;
    assert!(second_state.lease_store(&home("a")).await.is_err());
    let other = second_state.lease_store(&home("b")).await?;
    assert_ne!(first.record().store_id, other.record().store_id);
    state.close().await;
    drop(state);
    assert!(
        second_state.lease_store(&home("a")).await.is_err(),
        "state shutdown released an outstanding store lease"
    );
    drop(first);
    let replacement = second_state.lease_store(&home("a")).await?;
    assert_eq!(
        fs::metadata(&path)?.ino(),
        inode,
        "ownership lock was recreated"
    );
    replacement.revalidate().await?;
    second_state.close().await;
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn linux_posix_acl_is_refused_without_mode_changes_or_state_creation() -> Result<()> {
    use std::os::fd::AsRawFd;
    for name in [c"system.posix_acl_access", c"system.posix_acl_default"] {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("state");
        fs::create_dir(&root)?;
        chmod(&root, 0o700)?;
        let file = fs::File::open(&root)?;
        // Linux POSIX ACL xattr v2: owner, named user, group, mask, other.
        // Mask0 preserves0700 and grants the synthetic named user no access.
        let mut acl = 2_u32.to_le_bytes().to_vec();
        let named = file.metadata()?.uid().wrapping_add(1);
        for (tag, permissions, id) in [
            (1_u16, 7_u16, u32::MAX),
            (2, 7, named),
            (4, 0, u32::MAX),
            (16, 0, u32::MAX),
            (32, 0, u32::MAX),
        ] {
            acl.extend_from_slice(&tag.to_le_bytes());
            acl.extend_from_slice(&permissions.to_le_bytes());
            acl.extend_from_slice(&id.to_le_bytes());
        }
        // SAFETY: owned live directory fd, static name, initialized bounded ACL bytes.
        let status = unsafe {
            libc::fsetxattr(
                file.as_raw_fd(),
                name.as_ptr(),
                acl.as_ptr().cast(),
                acl.len(),
                0,
            )
        };
        anyhow::ensure!(
            status == 0,
            "install synthetic POSIX ACL: {}",
            std::io::Error::last_os_error()
        );
        assert_eq!(fs::metadata(&root)?.mode() & 0o777, 0o700);
        assert!(DurableState::open(&root).await.is_err());
        assert_eq!(fs::read_dir(&root)?.count(), 0);
        let mut after = vec![0_u8; acl.len()];
        // SAFETY: live fd/static name and a writable buffer of the supplied length.
        let size = unsafe {
            libc::fgetxattr(
                file.as_raw_fd(),
                name.as_ptr(),
                after.as_mut_ptr().cast(),
                after.len(),
            )
        };
        assert_eq!(size, acl.len() as isize);
        assert_eq!(
            after, acl,
            "private admission repaired an ACL without consent"
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn macos_allow_acl_is_refused_while_deny_only_acl_is_supported() -> Result<()> {
    for allow in [true, false] {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("state");
        fs::create_dir(&root)?;
        chmod(&root, 0o700)?;
        let rule = if allow {
            "everyone allow list,search"
        } else {
            "everyone deny delete"
        };
        let status = std::process::Command::new("/bin/chmod")
            .args(["+a", rule])
            .arg(&root)
            .status()?;
        anyhow::ensure!(status.success(), "install synthetic ACL fixture");
        let mode = fs::metadata(&root)?.mode() & 0o777;
        let opened = DurableState::open(&root).await;
        let entries = fs::read_dir(&root)?.count();
        // Remove only the ACL that this fixture installed, including on a red run.
        let status = std::process::Command::new("/bin/chmod")
            .arg("-N")
            .arg(&root)
            .status()?;
        anyhow::ensure!(status.success(), "remove synthetic ACL fixture");
        assert_eq!(mode, 0o700, "ACL fixture unexpectedly changed mode bits");
        if allow {
            assert!(opened.is_err(), "mode-only admission accepted an allow ACL");
            assert_eq!(
                entries, 0,
                "allow ACL was rejected only after state mutation"
            );
        } else {
            opened?.close().await;
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn macos_allow_acl_on_control_file_or_ancestor_blocks_catalog_writes() -> Result<()> {
    for on_parent in [false, true] {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("state");
        let state = DurableState::open(&root).await?;
        let target = if on_parent {
            temp.path().to_owned()
        } else {
            root.join("control/catalog-owner.json")
        };
        let rule = if on_parent {
            "everyone allow list,search"
        } else {
            "everyone allow read"
        };
        let database = root.join("control/catalog.sqlite3");
        let before = fs::read(&database)?;
        let status = std::process::Command::new("/bin/chmod")
            .args(["+a", rule])
            .arg(&target)
            .status()?;
        anyhow::ensure!(status.success(), "install synthetic control ACL");
        let result = state.lease_store(&home("not-yet-created")).await;
        let status = std::process::Command::new("/bin/chmod")
            .arg("-N")
            .arg(&target)
            .status()?;
        anyhow::ensure!(status.success(), "remove synthetic control ACL");
        assert!(
            result.is_err(),
            "unexpected ACL did not fence private state"
        );
        assert_eq!(
            fs::read(&database)?,
            before,
            "catalog changed before ACL admission"
        );
        assert_eq!(fs::read_dir(root.join("data"))?.count(), 0);
        state.close().await;
    }
    Ok(())
}

#[tokio::test]
async fn unsafe_root_and_unknown_contents_are_refused_without_repair() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("state");
    fs::create_dir(&root)?;
    chmod(&root, 0o755)?;
    assert!(DurableState::open(&root).await.is_err());
    assert_eq!(fs::read_dir(&root)?.count(), 0);
    assert_eq!(fs::metadata(&root)?.mode() & 0o777, 0o755);
    chmod(&root, 0o700)?;
    fs::write(root.join("unrelated"), "synthetic preserve-first sentinel")?;
    assert!(DurableState::open(&root).await.is_err());
    assert_eq!(fs::read_dir(&root)?.count(), 1);
    assert_eq!(
        fs::read_to_string(root.join("unrelated"))?,
        "synthetic preserve-first sentinel"
    );
    Ok(())
}

#[tokio::test]
async fn root_symlink_and_writable_parent_are_refused_before_creation() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let root = temp.path().join("state");
    std::os::unix::fs::symlink(outside.path(), &root)?;
    assert!(DurableState::open(&root).await.is_err());
    assert_eq!(fs::read_dir(outside.path())?.count(), 0);
    fs::remove_file(&root)?;
    chmod(temp.path(), 0o777)?;
    assert!(DurableState::open(&root).await.is_err());
    assert!(!root.exists());
    chmod(temp.path(), 0o700)?;
    Ok(())
}

#[tokio::test]
async fn control_symlinks_hardlinks_and_loose_modes_never_touch_the_target() -> Result<()> {
    for attack in ["symlink", "hardlink", "fifo", "mode"] {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("state");
        let state = DurableState::open(&root).await?;
        state.close().await;
        let target = temp.path().join("sentinel");
        fs::write(&target, "synthetic private sentinel")?;
        chmod(&target, 0o600)?;
        let sidecar = root.join("control/catalog.sqlite3-wal");
        match attack {
            "symlink" => std::os::unix::fs::symlink(&target, &sidecar)?,
            "hardlink" => fs::hard_link(&target, &sidecar)?,
            "fifo" => {
                let path = std::ffi::CString::new(sidecar.as_os_str().as_encoded_bytes())?;
                // SAFETY: NUL-terminated fixture path, fixed permissions, no retained pointer.
                let status = unsafe { libc::mkfifo(path.as_ptr(), 0o600) };
                anyhow::ensure!(
                    status == 0,
                    "create fixture FIFO: {}",
                    std::io::Error::last_os_error()
                );
            }
            _ => {
                fs::write(&sidecar, "synthetic loose sidecar")?;
                chmod(&sidecar, 0o644)?;
            }
        }
        let before = fs::read(root.join("control/catalog.sqlite3"))?;
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(2), DurableState::open(&root))
                .await?
                .is_err(),
            "accepted {attack}"
        );
        assert_eq!(fs::read_to_string(&target)?, "synthetic private sentinel");
        assert_eq!(fs::read(root.join("control/catalog.sqlite3"))?, before);
    }
    Ok(())
}

#[tokio::test]
async fn foreign_header_and_empty_existing_catalog_require_explicit_recovery() -> Result<()> {
    for bytes in [b"synthetic foreign catalog".as_slice(), b"".as_slice()] {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("state");
        let state = DurableState::open(&root).await?;
        let lease = state.lease_store(&home("a")).await?;
        let data = lease.data_directory().join("sentinel");
        fs::write(&data, "synthetic retained store")?;
        drop(lease);
        state.close().await;
        let db = root.join("control/catalog.sqlite3");
        let marker = fs::read(root.join("control/catalog-owner.json"))?;
        fs::write(&db, bytes)?;
        assert!(DurableState::open(&root).await.is_err());
        assert_eq!(fs::read(&db)?, bytes);
        assert_eq!(fs::read(root.join("control/catalog-owner.json"))?, marker);
        assert_eq!(fs::read_to_string(&data)?, "synthetic retained store");
    }
    Ok(())
}

#[tokio::test]
async fn replaced_control_directory_fences_leases_without_following_it() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    let root = temp.path().join("state");
    let state = DurableState::open(&root).await?;
    let lease = state.lease_store(&home("a")).await?;
    fs::rename(root.join("control/locks"), root.join("control/old-locks"))?;
    std::os::unix::fs::symlink(outside.path(), root.join("control/locks"))?;
    assert!(lease.revalidate().await.is_err());
    assert!(state.lease_store(&home("b")).await.is_err());
    assert_eq!(fs::read_dir(outside.path())?.count(), 0);
    state.close().await;
    Ok(())
}

#[tokio::test]
async fn missing_catalog_or_marker_never_adopts_or_replaces_retained_data() -> Result<()> {
    for missing in ["database", "marker", "both"] {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("state");
        let state = DurableState::open(&root).await?;
        let lease = state.lease_store(&home("a")).await?;
        let data = lease.data_directory().join("sentinel");
        fs::write(&data, "synthetic retained data")?;
        drop(lease);
        state.close().await;
        if missing != "marker" {
            fs::remove_file(root.join("control/catalog.sqlite3"))?;
        }
        if missing != "database" {
            fs::remove_file(root.join("control/catalog-owner.json"))?;
        }
        assert!(DurableState::open(&root).await.is_err());
        assert_eq!(
            root.join("control/catalog.sqlite3").exists(),
            missing == "marker"
        );
        assert_eq!(
            root.join("control/catalog-owner.json").exists(),
            missing == "database"
        );
        assert_eq!(fs::read_to_string(&data)?, "synthetic retained data");
    }
    Ok(())
}

#[tokio::test]
async fn stale_lease_drop_does_not_delete_or_unlock_a_new_lock_inode() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("state");
    let state = DurableState::open(&root).await?;
    let old = state.lease_store(&home("a")).await?;
    let path = root
        .join("control/locks")
        .join(format!("{}.lock", old.record().store_id));
    fs::rename(&path, path.with_extension("retired"))?;
    let new = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    new.try_lock()?;
    let inode = new.metadata()?.ino();
    assert!(old.revalidate().await.is_err());
    drop(old);
    assert_eq!(fs::metadata(&path)?.ino(), inode);
    assert!(state.lease_store(&home("a")).await.is_err());
    drop(new);
    state.lease_store(&home("a")).await?.revalidate().await?;
    state.close().await;
    Ok(())
}
