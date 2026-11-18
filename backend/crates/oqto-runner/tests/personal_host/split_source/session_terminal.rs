//! Restored Session PTY contract using only disposable HTTPS/PTY/source fixtures.
use super::*;

#[tokio::test]
async fn session_terminal_support_is_not_inferred_from_session_observation() -> Result<()> {
    let fixture = Fixture::start().await?;
    let host = &fixture.host;
    let device = host.enroll(&host.qr, 106).await?;
    let (status, caps) = host.action(&device, 106, HostAction::Capabilities).await?;
    ensure!(status == 200, "{caps}");
    assert_eq!(caps["session_observe_supported"], true);
    assert_eq!(caps["terminal_supported"], false);
    assert_eq!(caps["session_terminal_supported"], false);
    fixture.host.stop().await
}

async fn catalog_session(host: &Host, device: &str, seed: u8) -> Result<String> {
    let public = oqto_history::oqto_log::store::platform_id_for_external_id(NATIVE);
    // A transport Ping is not the serialized writer's header-adoption barrier.
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let (status, catalog) = host.action(device, seed, HostAction::SessionsList).await?;
            ensure!(status == 200 || status == 503, "{catalog}");
            if status == 200
                && catalog["sessions"]
                    .as_array()
                    .is_some_and(|rows| rows.iter().any(|row| row["id"] == public))
            {
                return Ok::<_, anyhow::Error>(public);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?
}

fn open(public: &str) -> HostAction {
    HostAction::SessionTerminalOpen {
        session_id: public.into(),
        rows: 24,
        cols: 80,
    }
}

async fn enroll_terminal(host: &Host, seed: u8, observe: bool) -> Result<String> {
    let mut args = vec!["pair", "--full-user-terminal", "--output", "json"];
    if observe {
        args.extend(["--sessions", "observe"]);
    }
    let qr = host.owner(&args).await?;
    let qr = PairingQr::parse(&serde_json::to_vec(&qr)?, now()?)?;
    host.enroll(&qr, seed).await
}

async fn assert_session_cwd(host: &Host, device: &str, seed: u8, id: &str) -> Result<()> {
    assert_eq!(
        host.action(
            device,
            seed,
            HostAction::TerminalWrite {
                terminal_id: id.into(),
                data_base64: STANDARD.encode(b"printf '\\nSYNTHETIC_CWD='; pwd\r"),
            }
        )
        .await?
        .0,
        200
    );
    let mut output = Vec::new();
    let mut after = 0;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let (status, page) = host
                .action(
                    device,
                    seed,
                    HostAction::TerminalRead {
                        terminal_id: id.into(),
                        after,
                    },
                )
                .await?;
            ensure!(status == 200, "{page}");
            output.extend(STANDARD.decode(page["data_base64"].as_str().context("PTY output")?)?);
            after = page["next"].as_u64().context("PTY cursor")?;
            if String::from_utf8_lossy(&output)
                .contains(&format!("SYNTHETIC_CWD={}", host.work.display()))
            {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await??;
    Ok(())
}

#[tokio::test]
async fn session_terminal_requires_both_grants_and_uses_authoritative_session_cwd() -> Result<()> {
    let fixture = Fixture::start_with_inventory(0, false, true).await?;
    fixture.primary().await?;
    let host = &fixture.host;
    let limited = host.enroll(&host.qr, 101).await?;
    let public = catalog_session(host, &limited, 101).await?;
    let (_, caps) = host.action(&limited, 101, HostAction::Capabilities).await?;
    assert_eq!(caps["session_terminal_supported"], true);
    assert_eq!(caps["full_user_terminal_granted"], false);
    assert_eq!(host.action(&limited, 101, open(&public)).await?.0, 403);
    let terminal_only = enroll_terminal(host, 102, false).await?;
    assert_eq!(
        host.action(&terminal_only, 102, open(&public)).await?.0,
        403
    );
    let device = enroll_terminal(host, 103, true).await?;
    for session in [
        "not-a-session".to_owned(),
        oqto_history::oqto_log::store::platform_id_for_external_id("outside-native"),
    ] {
        assert_eq!(host.action(&device, 103, open(&session)).await?.0, 403);
    }
    let (status, reply) = host.action(&device, 103, open(&public)).await?;
    ensure!(status == 200, "{reply}");
    assert_eq!(reply["session_id"], public);
    assert_eq!(reply["reattached"], false);
    let id = reply["terminal_id"].as_str().context("terminal ID")?;
    assert_session_cwd(host, &device, 103, id).await?;
    assert_eq!(
        host.action(
            &device,
            103,
            HostAction::TerminalClose {
                terminal_id: id.into()
            }
        )
        .await?
        .0,
        200
    );
    assert_eq!(host.owner(&["revoke", &device]).await?["revoked"], true);
    assert_eq!(host.action(&device, 103, open(&public)).await?.0, 401);
    assert_eq!(std::fs::read(&fixture.source)?, fixture.native_bytes);
    fixture.host.stop().await
}

#[tokio::test]
async fn session_terminal_concurrent_open_reuses_only_the_owning_devices_pty() -> Result<()> {
    let fixture = Fixture::start_with_inventory(0, false, true).await?;
    fixture.primary().await?;
    let host = &fixture.host;
    let device = enroll_terminal(host, 104, true).await?;
    let public = catalog_session(host, &device, 104).await?;
    let (first, second) = tokio::join!(
        host.action(&device, 104, open(&public)),
        host.action(&device, 104, open(&public))
    );
    let ((a, first), (b, second)) = (first?, second?);
    ensure!(a == 200 && b == 200, "{first} / {second}");
    assert_eq!(first["terminal_id"], second["terminal_id"]);
    assert_ne!(first["reattached"], second["reattached"]);
    let id = first["terminal_id"].as_str().context("terminal ID")?;
    let other = enroll_terminal(host, 105, true).await?;
    assert_eq!(
        host.action(
            &other,
            105,
            HostAction::TerminalRead {
                terminal_id: id.into(),
                after: 0
            }
        )
        .await?
        .0,
        403
    );
    let invalid = HostAction::SessionTerminalOpen {
        session_id: public.clone(),
        rows: 0,
        cols: 80,
    };
    assert_eq!(host.action(&device, 104, invalid).await?.0, 400);
    let (status, list) = host.action(&device, 104, HostAction::TerminalsList).await?;
    ensure!(status == 200, "{list}");
    assert_eq!(
        list["terminals"].as_array().context("terminal list")?.len(),
        1
    );
    let (status, foreign) = host.action(&other, 105, open(&public)).await?;
    ensure!(status == 200, "{foreign}");
    assert_ne!(first["terminal_id"], foreign["terminal_id"]);
    for (owner, seed, terminal) in [
        (&device, 104, id),
        (
            &other,
            105,
            foreign["terminal_id"].as_str().context("other terminal")?,
        ),
    ] {
        assert_eq!(
            host.action(
                owner,
                seed,
                HostAction::TerminalClose {
                    terminal_id: terminal.into()
                }
            )
            .await?
            .0,
            200
        );
    }
    assert_eq!(std::fs::read(&fixture.source)?, fixture.native_bytes);
    fixture.host.stop().await
}
