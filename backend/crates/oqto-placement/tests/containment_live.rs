#![cfg(target_os = "linux")]

use oqto_placement::{
    HostEndpointBridge, PlacementEndpoint, PlacementNetwork, PlacementNetworkMode, PlacementSpec,
    PlacementSupervisor, PodmanSupervisor,
};
use oqto_runner::transport::RunnerEndpointConfig;
use std::collections::BTreeMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn exec(container: &str, args: &[&str]) -> anyhow::Result<std::process::Output> {
    let output = std::process::Command::new("podman")
        .args(["exec", container])
        .args(args)
        .output()?;
    Ok(output)
}

/// Live containment proof for isolated workspace networks.
///
/// Run with:
/// `OQTO_WORKSPACE_IMAGE=localhost/oqto-workspace:dev cargo test -p oqto-placement --test containment_live -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires rootless Podman and a built Workspace image"]
async fn isolated_pod_reaches_only_granted_endpoints() -> anyhow::Result<()> {
    let image = std::env::var("OQTO_WORKSPACE_IMAGE")?;
    let temp = tempfile::tempdir()?;
    let workspace_dir = temp.path().join("workspace");
    let state_dir = temp.path().join("state");
    let runtime_dir = temp.path().join("runtime");
    std::fs::create_dir_all(&runtime_dir)?;

    // Host-side stand-in for a granted service (e.g. EAVS).
    let service = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let target = service.local_addr()?.to_string();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = service.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let _ = stream
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 10\r\n\r\nbridged-ok")
                    .await;
            });
        }
    });

    let _bridge =
        HostEndpointBridge::spawn(runtime_dir.join("endpoints").join("svc.sock"), target).await?;

    let spec = PlacementSpec {
        workspace_id: format!("contain-{}", std::process::id()),
        account_id: "containment-live-test".to_string(),
        image,
        workspace_dir,
        state_dir,
        runner_endpoint: RunnerEndpointConfig::Unix {
            path: runtime_dir.join("runner.sock"),
        },
        server_tls: None,
        environment: BTreeMap::new(),
        cpu_limit: Some("1".to_string()),
        memory_limit: Some("1g".to_string()),
        network: PlacementNetwork {
            mode: PlacementNetworkMode::Isolated,
            endpoints: vec![PlacementEndpoint {
                name: "svc".to_string(),
                port: 18080,
            }],
        },
    };

    let supervisor = PodmanSupervisor::new();
    let placement = supervisor.start(&spec).await?;
    let container = placement.runtime_name.clone();

    let proof = async {
        tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                if matches!(
                    supervisor.health(&placement).await?,
                    oqto_placement::PlacementHealth::Ready
                ) {
                    break Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
        })
        .await??;

        // Granted endpoint: reachable through the runner loopback bridge.
        let granted = exec(
            &container,
            &["curl", "-sS", "--max-time", "10", "http://127.0.0.1:18080/"],
        )?;
        anyhow::ensure!(
            granted.status.success()
                && String::from_utf8_lossy(&granted.stdout).contains("bridged-ok"),
            "granted endpoint not bridged: stdout={} stderr={}",
            String::from_utf8_lossy(&granted.stdout),
            String::from_utf8_lossy(&granted.stderr)
        );

        // Open internet: must be unreachable (no DNS, no route).
        let denied = exec(
            &container,
            &["curl", "-sS", "--max-time", "8", "https://example.com/"],
        )?;
        anyhow::ensure!(
            !denied.status.success(),
            "isolated workspace reached the public internet: {}",
            String::from_utf8_lossy(&denied.stdout)
        );

        // Raw-IP bypass: also unreachable (a well-known public resolver IP).
        let raw = exec(
            &container,
            &["curl", "-sS", "--max-time", "8", "http://1.1.1.1/"],
        )?;
        anyhow::ensure!(
            !raw.status.success(),
            "isolated workspace reached a raw public IP: {}",
            String::from_utf8_lossy(&raw.stdout)
        );

        // Host services that were NOT granted: unreachable (no route to host).
        let host_gw = exec(
            &container,
            &[
                "bash",
                "-c",
                "curl -sS --max-time 5 http://host.containers.internal:3033/ || curl -sS --max-time 5 http://10.0.2.2:3033/",
            ],
        )?;
        anyhow::ensure!(
            !host_gw.status.success(),
            "isolated workspace reached an ungranted host service: {}",
            String::from_utf8_lossy(&host_gw.stdout)
        );

        Ok::<_, anyhow::Error>(())
    }
    .await;

    if let Err(error) = &proof {
        let logs = std::process::Command::new("podman")
            .args(["logs", &container])
            .output()?;
        eprintln!(
            "containment proof failed: {error:#}\n--- runner logs ---\n{}",
            String::from_utf8_lossy(&logs.stderr)
        );
    }
    supervisor.stop(&placement).await?;
    proof
}
