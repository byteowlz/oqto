#![cfg(target_os = "linux")]

use oqto_placement::{PlacementSpec, PlacementSupervisor, PodmanSupervisor, runtime_name};
use oqto_runner::transport::RunnerEndpointConfig;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// PID1 correctness: `podman stop` must terminate the runner container
/// promptly and cleanly (signal reaches the runner through catatonit),
/// not time out into SIGKILL.
///
/// Run with:
/// `OQTO_WORKSPACE_IMAGE=localhost/oqto-workspace:dev cargo test -p oqto-placement --test pid1_live -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires rootless Podman and a built Workspace image"]
async fn podman_stop_terminates_runner_promptly() -> anyhow::Result<()> {
    let image = std::env::var("OQTO_WORKSPACE_IMAGE")?;
    let temp = tempfile::tempdir()?;
    let id = format!("pid1-{}", std::process::id());
    let runtime = temp.path().join("runtime");
    std::fs::create_dir_all(&runtime)?;

    let spec = PlacementSpec {
        workspace_id: id.clone(),
        account_id: "pid1-live-test".to_string(),
        image,
        workspace_dir: temp.path().join("workspace"),
        state_dir: temp.path().join("state"),
        runner_endpoint: RunnerEndpointConfig::Unix {
            path: runtime.join("runner.sock"),
        },
        server_tls: None,
        environment: BTreeMap::new(),
        cpu_limit: Some("1".to_string()),
        memory_limit: Some("1g".to_string()),
        network: Default::default(),
        userns: Default::default(),
    };
    let container = runtime_name(&id);

    let supervisor = PodmanSupervisor::new();
    let placement = supervisor.start(&spec).await?;

    let proof = async {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if matches!(
                    supervisor.health(&placement).await?,
                    oqto_placement::PlacementHealth::Ready
                ) {
                    break Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
        .await??;

        let started = Instant::now();
        let status = tokio::process::Command::new("podman")
            .args(["stop", "--time", "10", &container])
            .status()
            .await?;
        let elapsed = started.elapsed();
        anyhow::ensure!(status.success(), "podman stop failed");
        anyhow::ensure!(
            elapsed < Duration::from_secs(8),
            "stop took {elapsed:?}; runner likely SIGKILLed after timeout"
        );

        let exit_code = std::process::Command::new("podman")
            .args(["inspect", "--format", "{{.State.ExitCode}}", &container])
            .output()?;
        let code = String::from_utf8_lossy(&exit_code.stdout)
            .trim()
            .to_string();
        anyhow::ensure!(code == "0", "runner exited unclean: code {code}");
        Ok::<_, anyhow::Error>(())
    }
    .await;

    if proof.is_err() {
        let logs = std::process::Command::new("podman")
            .args(["logs", &container])
            .output()?;
        eprintln!(
            "pid1 proof failed\n--- runner logs ---\n{}",
            String::from_utf8_lossy(&logs.stderr)
        );
    }
    supervisor.stop(&placement).await?;
    proof
}
