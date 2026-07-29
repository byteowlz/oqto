#![cfg(target_os = "linux")]

use oqto_placement::{
    PlacementSpec, PlacementSupervisor, PlacementUserns, PodmanSupervisor, runtime_name,
};
use oqto_runner::transport::RunnerEndpointConfig;
use std::collections::BTreeMap;

fn spec_for(image: &str, id: &str, root: &std::path::Path) -> anyhow::Result<PlacementSpec> {
    let runtime = root.join(id).join("rsock");
    std::fs::create_dir_all(&runtime)?;
    Ok(PlacementSpec {
        workspace_id: id.to_string(),
        account_id: "subuid-live-test".to_string(),
        image: image.to_string(),
        workspace_dir: root.join(id).join("workspace"),
        state_dir: root.join(id).join("state"),
        runner_endpoint: RunnerEndpointConfig::Unix {
            path: runtime.join("runner.sock"),
        },
        server_tls: None,
        environment: BTreeMap::new(),
        cpu_limit: Some("1".to_string()),
        memory_limit: Some("1g".to_string()),
        network: Default::default(),
        userns: PlacementUserns::Auto { size: None },
    })
}

fn uid_map(container: &str) -> anyhow::Result<String> {
    let output = std::process::Command::new("podman")
        .args(["inspect", "--format", "{{.HostConfig.IDMappings.UIDMap}}"])
        .arg(container)
        .output()?;
    anyhow::ensure!(output.status.success(), "podman inspect failed");
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Live proof for per-workspace subuid isolation (auto userns).
///
/// Run with:
/// `OQTO_WORKSPACE_IMAGE=localhost/oqto-workspace:dev cargo test -p oqto-placement --test subuid_live -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires rootless Podman and a built Workspace image"]
async fn auto_userns_gives_disjoint_ranges_and_reachable_runners() -> anyhow::Result<()> {
    let image = std::env::var("OQTO_WORKSPACE_IMAGE")?;
    let temp = tempfile::tempdir()?;
    let root = temp.path().to_path_buf();

    let spec_a = spec_for(&image, &format!("subuid-a-{}", std::process::id()), &root)?;
    let spec_b = spec_for(&image, &format!("subuid-b-{}", std::process::id()), &root)?;

    let supervisor = PodmanSupervisor::new();
    let placement_a = supervisor.start(&spec_a).await?;
    let placement_b = supervisor.start(&spec_b).await?;

    let proof = async {
        for placement in [&placement_a, &placement_b] {
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
            .await??;
        }

        // Disjoint subuid ranges: the two containers must not share a mapping.
        let map_a = uid_map(&runtime_name(&spec_a.workspace_id))?;
        let map_b = uid_map(&runtime_name(&spec_b.workspace_id))?;
        anyhow::ensure!(
            map_a != map_b && !map_a.is_empty(),
            "expected disjoint uid maps, got {map_a} vs {map_b}"
        );

        // Volumes are chowned into the range: the backend user must no
        // longer own the state dir contents.
        let state_meta = std::fs::metadata(&spec_a.state_dir)?;
        use std::os::unix::fs::MetadataExt;
        anyhow::ensure!(
            state_meta.uid() != nix_geteuid(),
            "state dir still owned by the backend user; no subuid separation"
        );

        Ok::<_, anyhow::Error>(())
    }
    .await;

    if let Err(error) = &proof {
        for placement in [&placement_a, &placement_b] {
            let logs = std::process::Command::new("podman")
                .args(["logs", &placement.runtime_name])
                .output()?;
            eprintln!(
                "--- {} logs ---\n{}",
                placement.runtime_name,
                String::from_utf8_lossy(&logs.stderr)
            );
        }
        eprintln!("subuid proof failed: {error:#}");
    }
    supervisor.stop(&placement_a).await?;
    supervisor.stop(&placement_b).await?;

    // Chowned volumes cannot be deleted by the host user; clean up inside
    // the user namespace so the tempdir drop does not fail silently.
    let _ = std::process::Command::new("podman")
        .args(["unshare", "rm", "-rf"])
        .arg(temp.path())
        .status();
    std::mem::forget(temp);
    proof
}

fn nix_geteuid() -> u32 {
    // SAFETY: geteuid is always safe to call.
    unsafe { libc::geteuid() }
}
