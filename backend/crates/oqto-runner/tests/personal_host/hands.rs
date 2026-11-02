//! Real binary + pinned HTTPS + signed peer + private owner CLI + bwrap.
use super::*;

#[tokio::test]
#[ignore = "requires explicit compiled BPF and sandbox shim; run with OQT_HANDS_TEST_BPF"]
async fn consent_execution_dedup_and_revocation_over_https() -> Result<()> {
    let bpf = PathBuf::from(std::env::var("OQT_HANDS_TEST_BPF").context("set OQT_HANDS_TEST_BPF")?);
    let host = Host::start_options(false, Some(&bpf)).await?;
    let result = async {
        let id = host.enroll(&host.qr, 41).await?;
        let request = HostAction::HandsRequest {
            request_key: "one".into(),
            session_label: "disposable-session".into(),
            command: format!("printf one >> result; cat result; test -z \"${{OQTO_HANDS_FIXTURE_SECRET+x}}\" || exit 41; if cat '{}' >/dev/null 2>&1; then exit 42; fi", host.home.join("pi-sentinel").display()),
        };
        let (status, first) = host.action(&id, 41, request.clone()).await?;
        assert_eq!(status, 200);
        assert_eq!(first["state"], "pending");
        let invocation = first["id"].as_str().context("invocation")?;
        let (_, duplicate) = host.action(&id, 41, request).await?;
        assert_eq!(duplicate["id"], invocation);
        let (_, denied) = host
            .action(
                &id,
                41,
                HostAction::HandsRun {
                    id: invocation.into(),
                },
            )
            .await?;
        assert_eq!(denied["state"], "pending");
        assert!(!host.work.join("result").exists());
        let listing = host.owner(&["hands-requests"]).await?;
        assert!(
            listing["requests"]
                .as_array()
                .context("requests")?
                .iter()
                .any(|r| r["id"] == invocation)
        );
        let approved = host
            .owner(&[
                "hands-approve",
                invocation,
                "--root",
                host.work.to_str().context("workdir")?,
            ])
            .await?;
        assert_eq!(approved["applied"], true);
        host.action(
            &id,
            41,
            HostAction::HandsRun {
                id: invocation.into(),
            },
        )
        .await?;
        // A transport retry carries a fresh nonce but the SAME invocation.
        host.action(
            &id,
            41,
            HostAction::HandsRun {
                id: invocation.into(),
            },
        )
        .await?;
        let receipt = wait(&host, &id, 41, invocation).await?;
        assert_eq!(receipt["state"], "done", "{receipt}");
        assert_eq!(receipt["exit_code"], 0, "{receipt}");
        assert_eq!(std::fs::read_to_string(host.work.join("result"))?, "one");
        assert!(
            receipt["output"]
                .as_str()
                .context("output")?
                .contains("one")
        );
        let (_, slow) = host
            .action(
                &id,
                41,
                HostAction::HandsRequest {
                    request_key: "slow".into(),
                    session_label: "disposable-session".into(),
                    command: "printf started > started; sleep 2; printf unsafe > must-not-exist"
                        .into(),
                },
            )
            .await?;
        let slow = slow["id"].as_str().context("slow id")?;
        host.owner(&[
            "hands-approve",
            slow,
            "--root",
            host.work.to_str().context("workdir")?,
        ])
        .await?;
        host.action(&id, 41, HostAction::HandsRun { id: slow.into() })
            .await?;
        tokio::time::timeout(Duration::from_secs(5), async {
            while !host.work.join("started").exists() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await?;
        host.owner(&["hands-cancel", slow]).await?;
        tokio::time::sleep(Duration::from_millis(2300)).await;
        assert!(!host.work.join("must-not-exist").exists());
        let (_, cancelled) = host
            .action(&id, 41, HostAction::HandsStatus { id: slow.into() })
            .await?;
        assert_eq!(cancelled["state"], "cancelled");
        assert!(cancelled["output"].is_null());
        // Neither file grants nor an enrolled peer may approve itself.
        host.owner(&["revoke", &id]).await?;
        assert_ne!(
            host.action(
                &id,
                41,
                HostAction::HandsRun {
                    id: invocation.into()
                }
            )
            .await?
            .0,
            200
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if let Err(ref error) = result {
        eprintln!(
            "hands proof failed: {error:#}\n{}",
            std::fs::read_to_string(host._dir.path().join("host.stderr")).unwrap_or_default()
        );
    }
    host.stop().await?;
    result
}

async fn wait(host: &Host, device: &str, seed: u8, invocation: &str) -> Result<Value> {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let (status, receipt) = host
                .action(
                    device,
                    seed,
                    HostAction::HandsStatus {
                        id: invocation.into(),
                    },
                )
                .await?;
            ensure!(status == 200, "status request failed: {receipt}");
            if receipt["state"] != "running" {
                return Ok(receipt);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await?
}
