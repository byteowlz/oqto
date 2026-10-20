//! Deterministic readdir/stat race using only a disposable index sidecar.
use super::*;

static RETIRE_AFTER_ENUMERATION: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

pub(super) fn after_entry(entry: &std::fs::DirEntry) -> Result<()> {
    let mut armed = RETIRE_AFTER_ENUMERATION.lock().expect("sidecar hook");
    if armed.as_ref() == Some(&entry.path()) {
        assert!(entry.file_type()?.is_file());
        std::fs::remove_file(entry.path())?;
        *armed = None;
    }
    Ok(())
}

fn retire_next(path: &Path) -> Result<()> {
    std::fs::write(path, b"synthetic transient accelerator sidecar")?;
    *RETIRE_AFTER_ENUMERATION.lock().expect("sidecar hook") = Some(path.to_owned());
    Ok(())
}

fn assert_retired(path: &Path) {
    assert!(!path.exists());
    assert!(
        RETIRE_AFTER_ENUMERATION
            .lock()
            .expect("sidecar hook")
            .is_none()
    );
}

#[tokio::test]
async fn retired_index_sidecars_do_not_poison_global_identity_authority() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let workspace = "/synthetic/retained";
    let db = resource_tests::prefix_shard(temp.path(), workspace, 1).await?;
    let before = std::fs::read(&db)?;
    let root = db
        .parent()
        .context("shard parent")?
        .parent()
        .context("log root")?;
    // No index connection is opened in this test: these are synthetic regular
    // entries with the same type/name/lifetime boundary as real WAL/SHM files.
    for name in ["index.sqlite-shm", "index.sqlite-wal"] {
        let sidecar = root.join(name);
        retire_next(&sidecar)?;
        assert!(!is_bound(temp.path(), "retained").await?);
        assert_retired(&sidecar);

        retire_next(&sidecar)?;
        let mut batch = DiscoveryBatch::open(temp.path()).await?;
        assert_retired(&sidecar);
        retire_next(&sidecar)?;
        batch.prepare("retained").await?;
        assert_retired(&sidecar);
    }
    assert_eq!(std::fs::read(&db)?, before);
    // A genuine canonical shard without its DB must STILL refuse the whole
    // inventory. The fix must not turn missing authority into empty authority.
    std::fs::remove_file(&db)?;
    let error = is_bound(temp.path(), "retained")
        .await
        .expect_err("missing shard");
    assert!(authority_unavailable(&error));
    assert!(DiscoveryBatch::open(temp.path()).await.is_err());
    std::fs::write(&db, &before)?;
    assert!(!is_bound(temp.path(), "retained").await?);
    assert_eq!(std::fs::read(&db)?, before);
    #[cfg(unix)]
    {
        let broken = root.join("broken-shard-link");
        std::os::unix::fs::symlink(root.join("missing-shard"), &broken)?;
        let error = is_bound(temp.path(), "retained")
            .await
            .expect_err("broken shard link");
        assert!(authority_unavailable(&error));
        assert!(DiscoveryBatch::open(temp.path()).await.is_err());
        std::fs::remove_file(broken)?;
        assert!(!is_bound(temp.path(), "retained").await?);
    }
    Ok(())
}
