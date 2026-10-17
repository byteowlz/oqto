use super::*;

#[tokio::test]
async fn owner_qr_admits_only_signed_bounded_enrollment_with_ticket_grants() -> Result<()> {
    let (_dir, authority) = tests::fixture().await?;
    let qr = authority
        .issue_pairing_qr(
            &Grants::default(),
            "https://studio.local:7443",
            b"fixture-leaf",
        )
        .await?;
    assert_eq!(qr.host_id(), authority.host_id());
    assert!(qr.matches_leaf(b"fixture-leaf"));
    let scanned = PairingQr::parse(&qr.to_json()?, now()?)?;
    let ticket = PairingTicket {
        token: scanned.enrollment_token().into(),
        expires_at: scanned.expires_at(),
    };
    let proof = tests::enrollment(&ticket, 1);
    let mut widened = serde_json::to_value(&proof)?;
    widened["grants"] = serde_json::json!({"full_user_terminal":true});
    assert!(
        authority
            .enroll_json(&serde_json::to_vec(&widened)?)
            .await
            .is_err()
    );
    assert!(
        authority
            .enroll_json(&vec![b' '; oqto_personal_wire::MAX_ENROLLMENT_BYTES + 1])
            .await
            .is_err()
    );
    let encoded = serde_json::to_vec(&proof)?;
    let id = authority.enroll_json(&encoded).await?;
    assert!(authority.enroll_json(&encoded).await.is_err());
    let device = authority
        .authenticate(&tests::request(&id, 1, now()?))
        .await?;
    assert!(
        authority
            .require_current(&device, Access::FullUserTerminal)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn invalid_origin_does_not_mint_ticket() -> Result<()> {
    let (_dir, authority) = tests::fixture().await?;
    assert!(
        authority
            .issue_pairing_qr(&Grants::default(), "http://studio.local", b"fixture-leaf")
            .await
            .is_err()
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM personal_pairing_tickets")
        .fetch_one(&authority.pool)
        .await?;
    assert_eq!(count, 0);
    Ok(())
}
