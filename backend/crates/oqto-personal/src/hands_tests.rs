use super::*;
use anyhow::Result;

async fn device(authority: &PairingAuthority, seed: u8) -> Result<String> {
    let ticket = authority.issue_ticket(&Grants::default()).await?;
    authority.enroll(&tests::enrollment(&ticket, seed)).await
}
async fn fence(authority: &PairingAuthority, id: &str, seed: u8) -> Result<DeviceFence> {
    let authenticated = authority
        .authenticate(&tests::request(id, seed, now()?))
        .await?;
    Ok(authority
        .require_current(&authenticated, Access::Inventory)
        .await?
        .fence())
}

#[tokio::test]
async fn consent_is_exact_device_bound_and_claimed_once() -> Result<()> {
    let (dir, authority) = tests::fixture().await?;
    let id = device(&authority, 11).await?;
    let other = device(&authority, 12).await?;
    let request = authority
        .request_hands(&id, "call-1", "session", "echo hello")
        .await?;
    assert_eq!(request.state, "pending");
    assert!(
        authority
            .claim_hands(&id, &request.id, fence(&authority, &id, 11).await?)
            .await?
            .is_none()
    );
    let duplicate = authority
        .request_hands(&id, "call-1", "session", "echo hello")
        .await?;
    assert_eq!(duplicate.id, request.id);
    assert!(
        authority
            .request_hands(&id, "call-1", "session", "different")
            .await
            .is_err()
    );
    assert!(authority.hands_receipt(&other, &request.id).await.is_err());
    let root = dir.path().join("work");
    std::fs::create_dir(&root)?;
    assert!(authority.decide_hands(&request.id, Some(&root)).await?);
    let claim = authority
        .claim_hands(&id, &request.id, fence(&authority, &id, 11).await?)
        .await?
        .context("claim")?;
    assert!(
        authority
            .claim_hands(&id, &request.id, fence(&authority, &id, 11).await?)
            .await?
            .is_none()
    );
    authority.finish_hands(&claim, "hello", Some(0)).await?;
    let receipt = authority.hands_receipt(&id, &request.id).await?;
    assert_eq!(receipt.state, "done");
    assert_eq!(receipt.output.as_deref(), Some("hello"));
    Ok(())
}

#[tokio::test]
async fn revocation_and_cancellation_fence_output() -> Result<()> {
    let (dir, authority) = tests::fixture().await?;
    let id = device(&authority, 13).await?;
    let request = authority
        .request_hands(&id, "call", "session", "sleep 10")
        .await?;
    let root = dir.path().join("work");
    std::fs::create_dir(&root)?;
    authority.decide_hands(&request.id, Some(&root)).await?;
    let claim = authority
        .claim_hands(&id, &request.id, fence(&authority, &id, 13).await?)
        .await?
        .context("claim")?;
    assert!(authority.hands_active(&claim).await?);
    authority.revoke(&id).await?;
    assert!(!authority.hands_active(&claim).await?);
    authority
        .finish_hands(&claim, "must not leak", Some(0))
        .await?;
    let receipt = authority.hands_receipt(&id, &request.id).await?;
    assert_eq!(receipt.state, "uncertain");
    assert!(receipt.output.is_none());
    Ok(())
}

#[tokio::test]
async fn denial_and_expiry_cannot_be_approved_or_replayed() -> Result<()> {
    let (dir, authority) = tests::fixture().await?;
    let id = device(&authority, 14).await?;
    let request = authority
        .request_hands(&id, "call", "session", "echo no")
        .await?;
    authority.cancel_hands(Some(&id), &request.id).await?;
    let work = dir.path().join("work");
    std::fs::create_dir(&work)?;
    assert!(!authority.decide_hands(&request.id, Some(&work)).await?);
    assert!(
        authority
            .claim_hands(&id, &request.id, fence(&authority, &id, 14).await?)
            .await?
            .is_none()
    );
    let expired = authority
        .request_hands(&id, "expired", "session", "echo no")
        .await?;
    sqlx::query("UPDATE personal_hands SET expires_at=0 WHERE id=?")
        .bind(&expired.id)
        .execute(&authority.pool)
        .await?;
    assert!(!authority.decide_hands(&expired.id, Some(&work)).await?);
    assert_eq!(
        authority.hands_receipt(&id, &expired.id).await?.state,
        "denied"
    );
    Ok(())
}
