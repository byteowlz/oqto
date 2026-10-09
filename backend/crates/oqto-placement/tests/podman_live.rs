#![cfg(target_os = "linux")]

use oqto_placement::{PlacementSpec, PlacementSupervisor, PodmanSupervisor};
use oqto_runner::transport::RunnerEndpointConfig;
use std::collections::BTreeMap;

/// Live proof for the real rootless Podman adapter and Workspace runner image.
///
/// Run with:
/// `OQTO_WORKSPACE_IMAGE=localhost/oqto-workspace:dev cargo test -p oqto-placement --test podman_live -- --ignored --nocapture`
#[tokio::test]
#[ignore = "requires rootless Podman and a built Workspace image"]
async fn rootless_podman_runner_is_reachable_and_stoppable() -> anyhow::Result<()> {
    let image = std::env::var("OQTO_WORKSPACE_IMAGE")?;
    let temp = tempfile::tempdir()?;
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace_dir)?;
    std::fs::write(workspace_dir.join("durable.txt"), "runner-visible")?;
    let state_dir = temp.path().join("state");
    let runtime_dir = temp.path().join("runtime");
    std::fs::create_dir_all(&runtime_dir)?;

    let spec = PlacementSpec {
        workspace_id: format!("live-{}", std::process::id()),
        account_id: "placement-live-test".to_string(),
        image,
        workspace_dir: workspace_dir.clone(),
        state_dir,
        runner_endpoint: RunnerEndpointConfig::Unix {
            path: runtime_dir.join("runner.sock"),
        },
        server_tls: None,
        environment: BTreeMap::new(),
        cpu_limit: Some("1".to_string()),
        memory_limit: Some("1g".to_string()),
        network: Default::default(),
        userns: Default::default(),
    };

    let supervisor = PodmanSupervisor::new();
    let placement = supervisor.start(&spec).await?;
    let proof = async {
        let health = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let health = supervisor.health(&placement).await?;
                if matches!(health, oqto_placement::PlacementHealth::Ready) {
                    break Ok::<_, anyhow::Error>(health);
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
        })
        .await??;
        anyhow::ensure!(
            matches!(health, oqto_placement::PlacementHealth::Ready),
            "unexpected placement health: {health:?}"
        );

        // Workspace operations use the runner endpoint, never `podman exec`.
        let listing = oqto_placement::workspace_list_directory(&placement, &workspace_dir).await?;
        anyhow::ensure!(
            listing
                .entries
                .iter()
                .any(|entry| entry.name == "durable.txt"),
            "runner directory listing missed durable.txt"
        );
        let execution = oqto_placement::workspace_exec(
            &placement,
            &["/bin/printf".to_string(), "runner-exec-ok".to_string()],
        )
        .await?;
        anyhow::ensure!(
            execution.exit_code == 0 && execution.output.trim_end() == "runner-exec-ok"
        );

        let operator = oqto_placement::operator_for(&placement);
        let status = operator.status(&placement).await?;
        anyhow::ensure!(status.ready && status.backend == "rootless_podman");
        let inspection = operator.inspect(&placement).await?;
        anyhow::ensure!(inspection.backend == "rootless_podman");
        let logs = operator.logs(&placement, "200").await?;
        anyhow::ensure!(
            logs.contains("Runner listening"),
            "runner log was not exported"
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if let Err(error) = &proof {
        let logs = std::process::Command::new("podman")
            .args(["logs", &placement.runtime_name])
            .output()?;
        eprintln!("runner logs:\n{}", String::from_utf8_lossy(&logs.stderr));
        eprintln!("{}", String::from_utf8_lossy(&logs.stdout));
        eprintln!("placement error: {error:#}");
    }
    let stop = supervisor.stop(&placement).await;
    proof?;
    stop
}
