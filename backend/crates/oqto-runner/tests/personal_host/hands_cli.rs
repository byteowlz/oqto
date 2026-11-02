use super::*;

async fn peer(host: &Host, identity: &Path, args: &[&str]) -> Result<Value> {
    let output = command(&host.home)
        .args(["personal", "peer"])
        .args(args)
        .arg("--identity")
        .arg(identity)
        .output()
        .await?;
    ensure!(
        output.status.success(),
        "peer CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[tokio::test]
#[ignore = "requires compiled BPF and shim; explicit disposable HTTPS proof"]
async fn actual_peer_and_owner_clis_execute_a_remote_command() -> Result<()> {
    let bpf = PathBuf::from(std::env::var("OQT_HANDS_TEST_BPF").context("set OQT_HANDS_TEST_BPF")?);
    let host = Host::start_options(false, Some(&bpf)).await?;
    let result = async {
        let qr = host._dir.path().join("ticket.json");
        std::fs::write(&qr, host.qr.to_json()?)?;
        std::fs::set_permissions(&qr, std::fs::Permissions::from_mode(0o600))?;
        let credentials = host._dir.path().join("credentials");
        std::fs::create_dir(&credentials)?;
        std::fs::set_permissions(&credentials, std::fs::Permissions::from_mode(0o700))?;
        let identity = credentials.join("identity.json");
        let enrolled = peer(
            &host,
            &identity,
            &[
                "enroll",
                "--qr",
                qr.to_str().context("qr")?,
                "--name",
                "source-runner-fixture",
            ],
        )
        .await?;
        assert!(enrolled["device_id"].is_string());
        assert_eq!(
            std::fs::metadata(&identity)?.permissions().mode() & 0o777,
            0o600
        );
        let requested = peer(
            &host,
            &identity,
            &[
                "request",
                "--request-key",
                "cli-proof",
                "--session-label",
                "test-session",
                "--command",
                "printf remote-ok > cli-result; cat cli-result",
            ],
        )
        .await?;
        let id = requested["id"].as_str().context("request id")?;
        assert_eq!(requested["state"], "pending");
        host.owner(&[
            "hands-approve",
            id,
            "--root",
            host.work.to_str().context("work")?,
        ])
        .await?;
        peer(&host, &identity, &["run", id]).await?;
        let receipt = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let receipt = peer(&host, &identity, &["status", id]).await?;
                if receipt["state"] != "running" {
                    return Ok::<_, anyhow::Error>(receipt);
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await??;
        assert_eq!(receipt["state"], "done", "{receipt}");
        assert_eq!(receipt["exit_code"], 0, "{receipt}");
        assert_eq!(
            std::fs::read_to_string(host.work.join("cli-result"))?,
            "remote-ok"
        );
        assert!(
            receipt["output"]
                .as_str()
                .context("output")?
                .contains("remote-ok")
        );
        // The identity file is create-only, not a replaceable config target.
        let overwrite = command(&host.home)
            .args(["personal", "peer", "enroll", "--qr"])
            .arg(&qr)
            .args(["--identity"])
            .arg(&identity)
            .args(["--name", "overwrite"])
            .output()
            .await?;
        assert!(!overwrite.status.success());
        Ok::<_, anyhow::Error>(())
    }
    .await;
    host.stop().await?;
    result
}
