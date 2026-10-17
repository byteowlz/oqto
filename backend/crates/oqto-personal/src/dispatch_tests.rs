use super::*;
use std::future::Future;

#[tokio::test]
async fn revoke_is_fenced_against_already_admitted_dispatch() -> Result<()> {
    let (_dir, authority) = tests::fixture().await?;
    let grants = Grants {
        full_user_terminal: true,
        ..Grants::default()
    };
    let ticket = authority.issue_ticket(&grants).await?;
    let id = authority.enroll(&tests::enrollment(&ticket, 1)).await?;
    let device = authority
        .authenticate(&tests::request(&id, 1, now()?))
        .await?;
    let action = authority
        .require_current(&device, Access::FullUserTerminal)
        .await?;
    let mut revocation = std::pin::pin!(authority.revoke(&id));
    // Deterministically poll the queued writer once; no sleep/timing assertion.
    std::future::poll_fn(|context| {
        assert!(revocation.as_mut().poll(context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    assert!(action.grants().full_user_terminal);
    drop(action);
    assert!(revocation.await?);
    assert!(
        authority
            .require_current(&device, Access::FullUserTerminal)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn second_authority_cannot_bypass_the_dispatch_fence() -> Result<()> {
    let (dir, authority) = tests::fixture().await?;
    assert!(
        PairingAuthority::open_private(&dir.path().join("authority"))
            .await
            .is_err()
    );
    authority.pool.close().await;
    drop(authority);
    PairingAuthority::open_private(&dir.path().join("authority")).await?;
    Ok(())
}
