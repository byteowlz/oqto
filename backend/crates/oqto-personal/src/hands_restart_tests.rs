use super::*;
use anyhow::Result;

#[tokio::test]
async fn interrupted_claim_is_uncertain_after_restart_and_never_runs_twice() -> Result<()> {
    let (dir, authority) = tests::fixture().await?;
    let ticket = authority.issue_ticket(&Grants::default()).await?;
    let device = authority.enroll(&tests::enrollment(&ticket, 21)).await?;
    let request = authority
        .request_hands(&device, "restart", "session", "echo side-effect")
        .await?;
    let root = dir.path().join("work");
    std::fs::create_dir(&root)?;
    authority.decide_hands(&request.id, Some(&root)).await?;
    let authenticated = authority
        .authenticate(&tests::request(&device, 21, now()?))
        .await?;
    let fence = authority
        .require_current(&authenticated, Access::Inventory)
        .await?
        .fence();
    assert!(
        authority
            .claim_hands(&device, &request.id, fence)
            .await?
            .is_some()
    );
    authority.pool.close().await;
    drop(authority);
    let authority = PairingAuthority::open_private(&dir.path().join("authority")).await?;
    assert_eq!(
        authority.hands_receipt(&device, &request.id).await?.state,
        "uncertain"
    );
    let authenticated = authority
        .authenticate(&tests::request(&device, 21, now()?))
        .await?;
    let fence = authority
        .require_current(&authenticated, Access::Inventory)
        .await?
        .fence();
    assert!(
        authority
            .claim_hands(&device, &request.id, fence)
            .await?
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn another_device_cannot_cancel_or_claim_and_authority_root_cannot_be_granted() -> Result<()>
{
    let (dir, authority) = tests::fixture().await?;
    let ticket = authority.issue_ticket(&Grants::default()).await?;
    let a = authority.enroll(&tests::enrollment(&ticket, 22)).await?;
    let ticket = authority.issue_ticket(&Grants::default()).await?;
    let b = authority.enroll(&tests::enrollment(&ticket, 23)).await?;
    let request = authority
        .request_hands(&a, "key", "session", "echo no")
        .await?;
    assert!(!authority.cancel_hands(Some(&b), &request.id).await?);
    assert!(
        authority
            .decide_hands(&request.id, Some(dir.path()))
            .await
            .is_err()
    );
    let authenticated = authority
        .authenticate(&tests::request(&b, 23, now()?))
        .await?;
    let fence = authority
        .require_current(&authenticated, Access::Inventory)
        .await?
        .fence();
    assert!(
        authority
            .claim_hands(&b, &request.id, fence)
            .await?
            .is_none()
    );
    assert_eq!(
        authority.hands_receipt(&a, &request.id).await?.state,
        "pending"
    );
    Ok(())
}
