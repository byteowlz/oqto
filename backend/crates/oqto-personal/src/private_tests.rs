use super::*;

#[tokio::test]
async fn ownership_marker_uses_persistent_filesystem_identity() -> Result<()> {
    let (_dir, authority) = tests::fixture().await?;
    let marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(authority.root.join("store-owner.json"))?)?;
    assert_eq!(marker["purpose"], "oqto.personal-store.v2");
    assert!(
        marker["filesystem"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
    assert!(marker.get("dev").is_none());
    Ok(())
}

#[tokio::test]
async fn approved_legacy_migration_preserves_database_identity_and_backup() -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let (_dir, authority) = tests::fixture().await?;
    let root = authority.root.clone();
    let host = authority.host_id().to_owned();
    let leaf = authority.transport_identity().await?.leaf_der().to_vec();
    authority.pool.close().await;
    drop(authority);
    let db = root.join("personal.sqlite3");
    let marker = root.join("store-owner.json");
    let meta = db.metadata()?;
    let legacy = format!(
        "{{\"purpose\":\"oqto.personal-store.v1\",\"dev\":{},\"ino\":{}}}\n",
        meta.dev() ^ 1,
        meta.ino()
    );
    std::fs::write(&marker, &legacy)?;
    let before = std::fs::read(&db)?;
    assert!(PairingAuthority::open_private(&root).await.is_err());
    assert!(repair_store_owner(&root, &"0".repeat(64)).is_err());
    assert_eq!(std::fs::read_to_string(&marker)?, legacy);
    let digest = format!("{:x}", Sha256::digest(legacy.as_bytes()));
    let backup = repair_store_owner(&root, &digest)?;
    assert_eq!(std::fs::read_to_string(backup)?, legacy);
    assert_eq!(std::fs::read(&db)?, before);
    assert_eq!(db.metadata()?.ino(), meta.ino());
    let reopened = PairingAuthority::open_private(&root).await?;
    assert_eq!(reopened.host_id(), host);
    assert_eq!(reopened.transport_identity().await?.leaf_der(), leaf);
    assert!(
        repair_store_owner(&root, &digest).is_err(),
        "live authority must exclude repair"
    );
    assert!(restore_store_owner(&root, &digest).is_err());
    reopened.pool.close().await;
    drop(reopened);
    let before_restore = std::fs::read(&db)?;
    restore_store_owner(&root, &digest)?;
    assert_eq!(std::fs::read_to_string(&marker)?, legacy);
    assert_eq!(std::fs::read(&db)?, before_restore);
    assert!(PairingAuthority::open_private(&root).await.is_err());
    repair_store_owner(&root, &digest)?;
    assert!(PairingAuthority::open_private(&root).await.is_ok());
    Ok(())
}

#[tokio::test]
async fn explicit_repair_never_claims_foreign_inode() -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let (_dir, authority) = tests::fixture().await?;
    let root = authority.root.clone();
    authority.pool.close().await;
    drop(authority);
    let db = root.join("personal.sqlite3");
    let meta = db.metadata()?;
    let wrong = format!(
        "{{\"purpose\":\"oqto.personal-store.v1\",\"dev\":{},\"ino\":{}}}\n",
        meta.dev(),
        meta.ino() + 1
    );
    std::fs::write(root.join("store-owner.json"), &wrong)?;
    assert!(repair_store_owner(&root, &format!("{:x}", Sha256::digest(wrong.as_bytes()))).is_err());
    assert_eq!(
        std::fs::read_to_string(root.join("store-owner.json"))?,
        wrong
    );
    Ok(())
}
