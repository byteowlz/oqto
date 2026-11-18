use super::*;

fn changed(error: anyhow::Error) -> bool {
    error.downcast_ref::<GrantRootChanged>().is_some()
}

/// A grant as a stored v1 row would look after macOS renumbered the mount.
fn remounted(grant: &FileGrant) -> FileGrant {
    FileGrant {
        device: grant.device.wrapping_add(2),
        ..grant.clone()
    }
}

#[test]
fn unchanged_mount_opens_without_any_volume_record() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let grant = FileGrant::select(dir.path(), true)?;
    assert!(grant.open_root().is_ok());
    assert!(
        grant.same_mount_volume()?.is_some(),
        "same mount: recordable without approval"
    );
    assert!(grant.remounted_volume()?.is_none(), "nothing to repair");
    Ok(())
}

#[test]
fn remount_without_a_volume_record_needs_reapproval_as_a_typed_failure() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let stale = remounted(&FileGrant::select(dir.path(), true)?);
    assert!(
        changed(stale.open_root().unwrap_err()),
        "typed, not a generic fault"
    );
    assert!(
        stale.same_mount_volume()?.is_none(),
        "never silently trusted"
    );
    assert!(
        stale.remounted_volume()?.is_some(),
        "owner may approve: same path and inode"
    );
    Ok(())
}

#[test]
fn remount_with_the_recorded_volume_opens() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let grant = FileGrant::select(dir.path(), true)?;
    let volume = grant.same_mount_volume()?.expect("recordable");
    let mut stale = remounted(&grant);
    stale.with_volume(volume);
    assert!(
        stale.open_root().is_ok(),
        "the volume identity survives the remount"
    );
    Ok(())
}

#[test]
fn a_foreign_volume_or_a_different_folder_never_opens() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let grant = FileGrant::select(dir.path(), true)?;
    let volume = grant.same_mount_volume()?.expect("recordable");

    let mut foreign = remounted(&grant);
    foreign.with_volume("macos-volume:00000000-0000-4000-8000-000000000000".into());
    assert!(
        changed(foreign.open_root().unwrap_err()),
        "another volume is another folder"
    );

    let mut replaced = FileGrant {
        inode: grant.inode.wrapping_add(1),
        ..grant.clone()
    };
    replaced.with_volume(volume.clone());
    assert!(
        changed(replaced.open_root().unwrap_err()),
        "a replaced folder never matches"
    );
    assert!(
        replaced.remounted_volume()?.is_none(),
        "and is never repairable"
    );

    let moved = FileGrant {
        path: dir.path().join("missing"),
        ..remounted(&grant)
    };
    assert!(moved.remounted_volume().is_err() || moved.remounted_volume()?.is_none());
    Ok(())
}

#[test]
fn stored_json_keeps_the_v1_shape_for_rollback() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let mut grant = FileGrant::select(dir.path(), true)?;
    grant.with_volume("macos-volume:fixture".into());
    let json: serde_json::Value = serde_json::to_value(&grant)?;
    let mut keys: Vec<_> = json.as_object().expect("object").keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        ["device", "id", "inode", "path", "writable"],
        "an older Runner must still parse it"
    );
    Ok(())
}

/// Real remount on macOS: an APFS image mounted again after another volume took
/// its slot gets a new st_dev with the same volume UUID and inode. Ignored by
/// default (mounts disk images); run explicitly as qualification evidence.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "mounts APFS disk images; run with --ignored"]
fn real_apfs_remount_keeps_the_grant_with_its_recorded_volume() -> Result<()> {
    use std::process::Command;
    let work = tempfile::tempdir()?;
    let name = format!("OqtoGrant{}", std::process::id());
    let other = format!("{name}B");
    let image = |label: &str| -> Result<std::path::PathBuf> {
        let path = work.path().join(format!("{label}.dmg"));
        let status = Command::new("hdiutil")
            .args([
                "create", "-quiet", "-size", "20m", "-fs", "APFS", "-volname", label,
            ])
            .arg(&path)
            .status()?;
        ensure!(status.success(), "hdiutil create failed");
        Ok(path)
    };
    let attach = |path: &std::path::Path, label: &str| -> Result<std::path::PathBuf> {
        let status = Command::new("hdiutil")
            .args(["attach", "-quiet", "-nobrowse", "-noautoopen"])
            .arg(path)
            .status()?;
        ensure!(status.success(), "hdiutil attach failed");
        Ok(std::path::PathBuf::from("/Volumes").join(label))
    };
    let detach = |mount: &std::path::Path| {
        let _ = Command::new("hdiutil")
            .args(["detach", "-quiet"])
            .arg(mount)
            .status();
    };
    let (a, b) = (image(&name)?, image(&other)?);

    let mount = attach(&a, &name)?;
    let root = mount.join("granted");
    std::fs::create_dir_all(&root)?;
    let grant = FileGrant::select(&root, true)?;
    let volume = grant
        .same_mount_volume()?
        .expect("recordable on the original mount");
    detach(&mount);

    let shifted = attach(&b, &other)?;
    let remount = attach(&a, &name)?;
    let result = (|| -> Result<()> {
        let now = std::fs::metadata(remount.join("granted"))?;
        ensure!(
            now.dev() != grant.device,
            "fixture did not change st_dev; nothing proven"
        );
        assert!(
            changed(grant.open_root().unwrap_err()),
            "v1 check alone fails after a remount (the bug)"
        );
        let mut recorded = grant.clone();
        recorded.with_volume(volume.clone());
        recorded.open_root()?;
        Ok(())
    })();
    detach(&remount);
    detach(&shifted);
    result
}
