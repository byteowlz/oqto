#![cfg(target_os = "linux")]

use oqto_placement::{
    CONTAINER_HOME, PlacementSpec, PlacementSupervisor, PlacementUserns, PodmanSupervisor,
};
use oqto_runner::{client::RunnerClient, transport::RunnerEndpointConfig};
use std::collections::BTreeMap;
use std::time::Duration;

/// Post-start config writes must go through the runner: after `:U` chowns the
/// state volume into the container's subuid range, the backend user cannot
/// write it directly, but the runner (inside the userns) can.
///
/// Run with:
/// `OQTO_WORKSPACE_IMAGE=localhost/oqto-workspace:dev cargo test -p oqto-placement --test config_write_live -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires rootless Podman and a built Workspace image"]
async fn runner_writes_config_where_backend_cannot() -> anyhow::Result<()> {
    let image = std::env::var("OQTO_WORKSPACE_IMAGE")?;
    let temp = tempfile::tempdir()?;
    let id = format!("cfgw-{}", std::process::id());
    let runtime = temp.path().join("runtime");
    std::fs::create_dir_all(&runtime)?;

    let spec = PlacementSpec {
        workspace_id: id.clone(),
        account_id: "config-write-test".to_string(),
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
        userns: PlacementUserns::Auto { size: None },
    };

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

        // The backend user must NOT be able to write the chowned volume.
        let direct = std::fs::write(spec.state_dir.join("direct-write.txt"), "backend");
        anyhow::ensure!(
            direct.is_err(),
            "state volume is still backend-writable; auto-userns isolation is not in effect"
        );

        // The runner must be able to write the same volume.
        let runner = RunnerClient::from_endpoint(&placement.runner_endpoint)?;
        runner.ensure_ready_with_recovery().await?;
        let models_path = std::path::Path::new(CONTAINER_HOME)
            .join(".pi")
            .join("agent")
            .join("models.json");
        let payload = br#"{"providers":{"eavs":{"apiKey":"test-key-123"}}}"#;
        let written = runner.write_file(&models_path, payload, true).await?;
        anyhow::ensure!(written.bytes_written == payload.len() as u64);

        // Round-trip through the runner proves durable, correct content.
        use base64::Engine;
        let read = runner.read_file(&models_path, None, None).await?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(read.content_base64)?;
        anyhow::ensure!(bytes == payload, "models.json round-trip mismatch");
        Ok::<_, anyhow::Error>(())
    }
    .await;

    if proof.is_err() {
        let logs = std::process::Command::new("podman")
            .args(["logs", &placement.runtime_name])
            .output()?;
        eprintln!("runner logs:\n{}", String::from_utf8_lossy(&logs.stderr));
    }
    let stop = supervisor.stop(&placement).await;
    let _ = std::process::Command::new("podman")
        .args(["unshare", "rm", "-rf"])
        .arg(temp.path())
        .status();
    std::mem::forget(temp);
    proof?;
    stop
}
