use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use std::os::unix::fs::{PermissionsExt, symlink};
use tempfile::TempDir;

pub(super) async fn fixture() -> Result<(TempDir, PairingAuthority)> {
    let dir = tempfile::tempdir()?;
    let authority = PairingAuthority::open_private(&dir.path().join("authority")).await?;
    Ok((dir, authority))
}
pub(super) fn enrollment(ticket: &PairingTicket, seed: u8) -> Enrollment {
    let key = SigningKey::from_bytes(&[seed; 32]);
    let mut proof = Enrollment {
        token: ticket.token.clone(),
        name: "Fixture phone".into(),
        public_key: STANDARD.encode(key.verifying_key().to_bytes()),
        signature: String::new(),
    };
    proof.signature = STANDARD.encode(
        key.sign(&proof.signing_bytes_for(OQTO_PERSONAL_V1))
            .to_bytes(),
    );
    proof
}
pub(super) fn request(id: &str, seed: u8, timestamp: i64) -> SignedRequest {
    let key = SigningKey::from_bytes(&[seed; 32]);
    let mut request = SignedRequest {
        device_id: id.into(),
        timestamp,
        nonce: Uuid::new_v4().to_string(),
        payload: "{\"op\":\"fixture\"}".into(),
        signature: String::new(),
    };
    request.signature = STANDARD.encode(
        key.sign(&request.signing_bytes_for(OQTO_PERSONAL_V1))
            .to_bytes(),
    );
    request
}

#[tokio::test]
async fn enrollment_is_single_use_and_cannot_supply_its_own_grants() -> Result<()> {
    let (_dir, authority) = fixture().await?;
    let ticket = authority.issue_ticket(&Grants::default()).await?;
    let proof = enrollment(&ticket, 1);
    let mut altered = serde_json::to_value(&proof)?;
    altered["grants"] = serde_json::json!({"full_user_terminal":true});
    assert!(serde_json::from_value::<Enrollment>(altered).is_err());
    let id = authority.enroll(&proof).await?;
    assert!(authority.enroll(&proof).await.is_err());
    let device = authority.authenticate(&request(&id, 1, now()?)).await?;
    assert!(
        authority
            .require_current(&device, Access::FullUserTerminal)
            .await
            .is_err()
    );
    assert!(
        authority
            .require_current(&device, Access::SessionObserve)
            .await
            .is_err()
    );
    let stored: String = sqlx::query_scalar("SELECT token_hash FROM personal_pairing_tickets")
        .fetch_one(&authority.pool)
        .await?;
    assert_ne!(stored, ticket.token);
    Ok(())
}

#[tokio::test]
async fn concurrent_enrollment_has_exactly_one_winner() -> Result<()> {
    let (_dir, authority) = fixture().await?;
    let ticket = authority.issue_ticket(&Grants::default()).await?;
    let a = enrollment(&ticket, 1);
    let b = enrollment(&ticket, 2);
    let (a, b) = tokio::join!(authority.enroll(&a), authority.enroll(&b));
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    Ok(())
}

#[tokio::test]
async fn bad_proof_does_not_consume_ticket_and_expired_ticket_is_denied() -> Result<()> {
    let (_dir, authority) = fixture().await?;
    let ticket = authority.issue_ticket(&Grants::default()).await?;
    let mut bad = enrollment(&ticket, 1);
    bad.name = "changed".into();
    assert!(authority.enroll(&bad).await.is_err());
    authority.enroll(&enrollment(&ticket, 1)).await?;
    let expired = authority.issue_ticket(&Grants::default()).await?;
    sqlx::query("UPDATE personal_pairing_tickets SET expires_at=0 WHERE token_hash=?")
        .bind(token_hash(&expired.token))
        .execute(&authority.pool)
        .await?;
    assert!(authority.enroll(&enrollment(&expired, 2)).await.is_err());
    Ok(())
}

#[tokio::test]
async fn proofs_and_nonces_do_not_cross_keys_domains_or_replay() -> Result<()> {
    let (_dir, authority) = fixture().await?;
    let ticket = authority.issue_ticket(&Grants::default()).await?;
    let id = authority.enroll(&enrollment(&ticket, 1)).await?;
    assert!(
        authority
            .authenticate(&request(&id, 2, now()?))
            .await
            .is_err()
    );
    let mut cross = request(&id, 1, now()?);
    cross.signature = STANDARD.encode(
        SigningKey::from_bytes(&[1; 32])
            .sign(&cross.signing_bytes())
            .to_bytes(),
    );
    assert!(authority.authenticate(&cross).await.is_err());
    let request = request(&id, 1, now()?);
    authority.authenticate(&request).await?;
    assert!(authority.authenticate(&request).await.is_err());
    Ok(())
}

#[tokio::test]
async fn grants_are_independent_and_revocation_invalidates_existing_context() -> Result<()> {
    let (_dir, authority) = fixture().await?;
    let files = tempfile::tempdir()?;
    let file = FileGrant::select(files.path(), false)?;
    let root_id = file.id().to_owned();
    let grants = Grants {
        sessions: SessionAccess::None,
        files: vec![file],
        full_user_terminal: true,
    };
    let ticket = authority.issue_ticket(&grants).await?;
    let id = authority.enroll(&enrollment(&ticket, 1)).await?;
    let device = authority.authenticate(&request(&id, 1, now()?)).await?;
    authority
        .require_current(&device, Access::FullUserTerminal)
        .await?;
    authority
        .require_current(&device, Access::FileRead(&root_id))
        .await?;
    for access in [
        Access::SessionObserve,
        Access::SessionControl,
        Access::FileWrite(&root_id),
        Access::FileRead("unknown-root"),
    ] {
        assert!(authority.require_current(&device, access).await.is_err());
    }
    assert!(authority.revoke(&id).await?);
    assert!(
        authority
            .require_current(&device, Access::FullUserTerminal)
            .await
            .is_err()
    );
    assert!(
        authority
            .authenticate(&request(&id, 1, now()?))
            .await
            .is_err()
    );
    assert!(!authority.revoke(&id).await?);
    Ok(())
}

#[tokio::test]
async fn canonical_store_survives_reopen_with_nonce_and_grants_intact() -> Result<()> {
    let (dir, authority) = fixture().await?;
    let grants = Grants {
        sessions: SessionAccess::Control,
        ..Grants::default()
    };
    let ticket = authority.issue_ticket(&grants).await?;
    let proof = enrollment(&ticket, 1);
    let id = authority.enroll(&proof).await?;
    let request = request(&id, 1, now()?);
    let device = authority.authenticate(&request).await?;
    authority.pool.close().await;
    drop(authority);
    let reopened = PairingAuthority::open_private(&dir.path().join("authority")).await?;
    assert!(reopened.enroll(&proof).await.is_err());
    assert!(reopened.authenticate(&request).await.is_err());
    reopened
        .require_current(&device, Access::SessionControl)
        .await?;
    reopened
        .require_current(&device, Access::SessionObserve)
        .await?;
    assert!(
        reopened
            .require_current(&device, Access::FullUserTerminal)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn clock_rollback_cannot_revive_a_pruned_nonce() -> Result<()> {
    let (_dir, authority) = fixture().await?;
    let ticket = authority.issue_ticket(&Grants::default()).await?;
    let id = authority.enroll(&enrollment(&ticket, 1)).await?;
    let first_time = now()?;
    let old = request(&id, 1, first_time);
    authority.authenticate_at(&old, first_time).await?;
    let future = request(&id, 1, first_time + 300);
    authority.authenticate_at(&future, first_time + 300).await?;
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM personal_request_nonces WHERE nonce=?")
            .bind(&old.nonce)
            .fetch_one(&authority.pool)
            .await?;
    assert_eq!(count, 0);
    assert!(authority.authenticate_at(&old, first_time).await.is_err());
    Ok(())
}

#[tokio::test]
async fn private_state_permissions_and_unknown_database_are_preserved() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().join("state");
    std::fs::create_dir(&root)?;
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755))?;
    assert!(PairingAuthority::open_private(&root).await.is_err());
    assert_eq!(root.metadata()?.permissions().mode() & 0o777, 0o755);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
    let database = root.join("personal.sqlite3");
    std::fs::write(&database, b"not our database; must stay unchanged")?;
    std::fs::set_permissions(&database, std::fs::Permissions::from_mode(0o600))?;
    assert!(PairingAuthority::open_private(&root).await.is_err());
    assert_eq!(
        std::fs::read(&database)?,
        b"not our database; must stay unchanged"
    );
    std::fs::remove_file(&database)?;
    symlink(dir.path().join("outside"), &database)?;
    assert!(PairingAuthority::open_private(&root).await.is_err());
    Ok(())
}

#[tokio::test]
async fn file_root_identity_and_private_authority_overlap_fail_closed() -> Result<()> {
    let (dir, authority) = fixture().await?;
    let parent = FileGrant::select(dir.path(), true)?;
    assert!(
        authority
            .issue_ticket(&Grants {
                files: vec![parent],
                ..Grants::default()
            })
            .await
            .is_err()
    );
    let selected = dir.path().join("selected");
    std::fs::create_dir(&selected)?;
    let grant = FileGrant::select(&selected, true)?;
    let handle = grant.open_root()?;
    std::fs::rename(&selected, dir.path().join("original"))?;
    std::fs::create_dir(&selected)?;
    assert!(grant.open_root().is_err());
    assert!(handle.metadata()?.is_dir());
    Ok(())
}

#[tokio::test]
async fn signed_control_character_label_is_denied_without_consuming_ticket() -> Result<()> {
    let (_dir, authority) = fixture().await?;
    let ticket = authority.issue_ticket(&Grants::default()).await?;
    let mut proof = enrollment(&ticket, 1);
    proof.name = "\x1b[2Jphone".into();
    proof.signature = STANDARD.encode(
        SigningKey::from_bytes(&[1; 32])
            .sign(&proof.signing_bytes_for(OQTO_PERSONAL_V1))
            .to_bytes(),
    );
    assert!(authority.enroll(&proof).await.is_err());
    authority.enroll(&enrollment(&ticket, 1)).await?;
    Ok(())
}
