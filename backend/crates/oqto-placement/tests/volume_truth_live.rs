#![cfg(target_os = "linux")]

use oqto_placement::{
    PlacementSpec, PlacementSupervisor, PlacementUserns, PodmanSupervisor, runtime_name,
};
use oqto_runner::transport::RunnerEndpointConfig;
use std::collections::BTreeMap;

fn exec(container: &str, args: &[&str]) -> anyhow::Result<std::process::Output> {
    Ok(std::process::Command::new("podman")
        .args(["exec", container])
        .args(args)
        .output()?)
}

async fn wait_ready(
    supervisor: &PodmanSupervisor,
    placement: &oqto_placement::PlacementRecord,
) -> anyhow::Result<()> {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            if matches!(
                supervisor.health(placement).await?,
                oqto_placement::PlacementHealth::Ready
            ) {
                break Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    })
    .await?
}

/// Container = ephemeral compute, volume = durable truth: destroying and
/// recreating the Pod must preserve Pi session JSONL, mmry data, oqto-log
/// state, tool installs, and workspace files — including under auto userns
/// where the recreated container maps a different subuid range.
///
/// Run with:
/// `OQTO_WORKSPACE_IMAGE=localhost/oqto-workspace:dev cargo test -p oqto-placement --test volume_truth_live -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires rootless Podman and a built Workspace image"]
async fn volume_survives_pod_destruction_and_recreation() -> anyhow::Result<()> {
    let image = std::env::var("OQTO_WORKSPACE_IMAGE")?;
    let temp = tempfile::tempdir()?;
    let id = format!("volume-{}", std::process::id());
    let runtime = temp.path().join(&id).join("rsock");
    std::fs::create_dir_all(&runtime)?;

    let spec = PlacementSpec {
        workspace_id: id.clone(),
        account_id: "volume-live-test".to_string(),
        image,
        workspace_dir: temp.path().join(&id).join("workspace"),
        state_dir: temp.path().join(&id).join("state"),
        runner_endpoint: RunnerEndpointConfig::Unix {
            path: runtime.join("runner.sock"),
        },
        server_tls: None,
        environment: BTreeMap::new(),
        cpu_limit: Some("1".to_string()),
        memory_limit: Some("1g".to_string()),
        network: Default::default(),
        userns: PlacementUserns::Auto { size: None },
    };
    let container = runtime_name(&id);

    let supervisor = PodmanSupervisor::new();
    let placement = supervisor.start(&spec).await?;

    let proof = async {
        wait_ready(&supervisor, &placement).await?;

        // Durable state across every mapped location.
        let seed = exec(
            &container,
            &[
                "bash",
                "-c",
                "set -e; \
                 mkdir -p ~/.pi/agent/sessions/proj ~/.local/share/mmry ~/.local/state/oqto ~/.cargo/bin /workspace/repo; \
                 echo '{\"session\":\"sacred\"}' > ~/.pi/agent/sessions/proj/s1.jsonl; \
                 echo memory > ~/.local/share/mmry/m.db; \
                 echo history > ~/.local/state/oqto/log.sqlite; \
                 printf '#!/bin/sh\\necho tool-ok\\n' > ~/.cargo/bin/mytool; chmod +x ~/.cargo/bin/mytool; \
                 echo code > /workspace/repo/main.rs",
            ],
        )?;
        anyhow::ensure!(
            seed.status.success(),
            "seeding failed: {}",
            String::from_utf8_lossy(&seed.stderr)
        );

        // Destroy the compute.
        supervisor.stop(&placement).await?;

        // Recreate: new container, new subuid range, same volumes.
        let placement2 = supervisor.start(&spec).await?;
        wait_ready(&supervisor, &placement2).await?;

        let verify = exec(
            &container,
            &[
                "bash",
                "-c",
                "set -e; \
                 grep -q sacred ~/.pi/agent/sessions/proj/s1.jsonl; \
                 grep -q memory ~/.local/share/mmry/m.db; \
                 grep -q history ~/.local/state/oqto/log.sqlite; \
                 test \"$(~/.cargo/bin/mytool)\" = tool-ok; \
                 grep -q code /workspace/repo/main.rs; \
                 echo all-durable",
            ],
        )?;
        anyhow::ensure!(
            verify.status.success()
                && String::from_utf8_lossy(&verify.stdout).contains("all-durable"),
            "durability verification failed: stdout={} stderr={}",
            String::from_utf8_lossy(&verify.stdout),
            String::from_utf8_lossy(&verify.stderr)
        );

        supervisor.stop(&placement2).await?;
        Ok::<_, anyhow::Error>(())
    }
    .await;

    if proof.is_err() {
        let logs = std::process::Command::new("podman")
            .args(["logs", &container])
            .output()?;
        eprintln!(
            "volume proof failed\n--- runner logs ---\n{}",
            String::from_utf8_lossy(&logs.stderr)
        );
        let _ = std::process::Command::new("podman")
            .args(["pod", "rm", "--force", &format!("{container}-pod")])
            .status();
    }

    let _ = std::process::Command::new("podman")
        .args(["unshare", "rm", "-rf"])
        .arg(temp.path())
        .status();
    std::mem::forget(temp);
    proof
}
