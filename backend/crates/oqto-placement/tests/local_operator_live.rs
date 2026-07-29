#![cfg(target_os = "linux")]

use oqto_placement::{LocalProcessSupervisor, PlacementSpec, PlacementSupervisor};
use oqto_runner::transport::RunnerEndpointConfig;
use std::collections::BTreeMap;
use std::time::Duration;

/// Proves the same Workspace operator path against LocalProcess placement.
/// Run with:
/// `OQTO_RUNNER_BINARY=target/debug/oqto-runner cargo test -p oqto-placement --test local_operator_live -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a built oqto-runner binary"]
async fn local_process_supports_neutral_status_exec_and_files() -> anyhow::Result<()> {
    let runner = std::env::var("OQTO_RUNNER_BINARY")?;
    let temp = tempfile::tempdir()?;
    let workspace_dir = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace_dir)?;
    std::fs::write(workspace_dir.join("local.txt"), "local")?;
    let spec = PlacementSpec {
        workspace_id: format!("local-live-{}", std::process::id()),
        account_id: "local-live-account".to_string(),
        image: "unused-for-local-process".to_string(),
        workspace_dir: workspace_dir.clone(),
        state_dir: temp.path().join("state"),
        runner_endpoint: RunnerEndpointConfig::Unix {
            path: temp.path().join("runtime/runner.sock"),
        },
        server_tls: None,
        environment: BTreeMap::new(),
        cpu_limit: None,
        memory_limit: None,
        network: Default::default(),
        userns: Default::default(),
    };
    std::fs::create_dir_all(&spec.state_dir)?;

    let supervisor = LocalProcessSupervisor::new(runner);
    let placement = supervisor.start(&spec).await?;
    let proof = async {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let status = oqto_placement::operator_for(&placement)
                    .status(&placement)
                    .await?;
                if status.ready {
                    break Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        })
        .await??;

        let listing = oqto_placement::workspace_list_directory(&placement, &workspace_dir).await?;
        anyhow::ensure!(
            listing
                .entries
                .iter()
                .any(|entry| entry.name == "local.txt")
        );
        let execution = oqto_placement::workspace_exec(
            &placement,
            &["/bin/printf".to_string(), "local-exec-ok".to_string()],
        )
        .await?;
        anyhow::ensure!(
            execution.exit_code == 0 && execution.output.trim_end() == "local-exec-ok",
            "unexpected execution: {execution:?}"
        );

        let inspection = oqto_placement::operator_for(&placement)
            .inspect(&placement)
            .await?;
        anyhow::ensure!(inspection.backend == "local_process");
        Ok::<_, anyhow::Error>(())
    }
    .await;

    let stop = supervisor.stop(&placement).await;
    proof?;
    stop
}
