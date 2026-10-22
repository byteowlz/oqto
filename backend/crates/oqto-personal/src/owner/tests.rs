use super::*;

#[tokio::test]
async fn disabled_terminal_is_definitive_over_owner_wire_and_never_mints_ticket() -> Result<()> {
    let (_dir, authority) = crate::tests::fixture().await?;
    let authority = Arc::new(authority);
    let listener = authority.bind_owner_socket()?;
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(serve_owner(
        listener,
        authority.clone(),
        "https://fixture.test".into(),
        vec![],
        Supported {
            files: true,
            terminal: false,
            sessions: true,
            control: true,
        },
        async {
            let _ = stopped.await;
        },
    ));
    let result = request_owner(
        &authority.root,
        &OwnerCommand::Pair {
            sessions: SessionAccess::Control,
            files: vec![],
            full_user_terminal: true,
        },
    )
    .await;
    let _ = stop.send(());
    server.await??;
    let error = result
        .err()
        .context("disabled terminal unexpectedly accepted")?;
    ensure!(matches!(
        error.downcast_ref::<OwnerRequestRejected>(),
        Some(OwnerRequestRejected::TerminalUnavailable)
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM personal_pairing_tickets")
        .fetch_one(&authority.pool)
        .await?;
    ensure!(count == 0, "rejected request minted a ticket");
    Ok(())
}
