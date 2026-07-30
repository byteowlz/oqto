#![cfg(target_os = "linux")]

//! Live proof for placement-neutral port exposure (oqto-5ae3.3): inside an
//! auto-userns, network=none workspace container, the runner starts a real
//! session (oqto-files + ttyd), exposes the fileserver port as a reverse
//! bridge socket, and a >100MB streamed multipart upload through that socket
//! lands atomically in the workspace volume. Ungranted network stays dead.
//!
//! Run with:
//! `OQTO_WORKSPACE_IMAGE=localhost/oqto-workspace:dev cargo test -p oqto-placement --test expose_port_live -- --ignored --nocapture`

use anyhow::Context;
use oqto_placement::{PlacementSpec, PlacementSupervisor, PlacementUserns, PodmanSupervisor};
use oqto_runner::protocol::ExposedEndpoint;
use oqto_runner::transport::RunnerEndpointConfig;
use std::collections::BTreeMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// >100MB (decimal) proof bar, within the shipped 100MiB files.toml limit.
const UPLOAD_BYTES: usize = 101_000_000;
const FILESERVER_PORT: u16 = 4101;
const TTYD_PORT: u16 = 4102;

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires rootless Podman and a built Workspace image"]
async fn exposed_fileserver_socket_carries_streamed_upload() -> anyhow::Result<()> {
    let image = std::env::var("OQTO_WORKSPACE_IMAGE")?;
    let temp = tempfile::tempdir()?;
    let id = format!("expose-{}", std::process::id());
    let runtime = temp.path().join(&id).join("rsock");
    std::fs::create_dir_all(&runtime)?;
    let workspace_dir = temp.path().join(&id).join("workspace");
    std::fs::create_dir_all(&workspace_dir)?;

    let spec = PlacementSpec {
        workspace_id: id.clone(),
        account_id: "expose-live-test".to_string(),
        image,
        workspace_dir: workspace_dir.clone(),
        state_dir: temp.path().join(&id).join("state"),
        runner_endpoint: RunnerEndpointConfig::Unix {
            path: runtime.join("runner.sock"),
        },
        server_tls: None,
        environment: BTreeMap::new(),
        cpu_limit: Some("2".to_string()),
        memory_limit: Some("1g".to_string()),
        network: Default::default(),
        userns: PlacementUserns::Auto { size: None },
    };

    let supervisor = PodmanSupervisor::new();
    let placement = supervisor.start(&spec).await?;

    let proof = async {
        wait_ready(&supervisor, &placement).await?;

        let client = oqto_runner::client::RunnerClient::new(runtime.join("runner.sock"));
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                if client.ensure_ready_with_recovery().await.is_ok() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
        })
        .await
        .context("runner not ready")?;

        // Real session: runner spawns oqto-files and ttyd on container loopback.
        client
            .start_session(
                "expose-live",
                "/workspace",
                4100,
                FILESERVER_PORT,
                TTYD_PORT,
                None,
                Default::default(),
            )
            .await
            .context("start_session")?;

        // Expose the fileserver port; must come back as a Unix socket that
        // the host reaches through the bind-mounted rsock directory.
        let endpoint = client.expose_port(FILESERVER_PORT).await?;
        let ExposedEndpoint::Unix { path } = endpoint else {
            anyhow::bail!("expected unix endpoint in container placement, got {endpoint:?}");
        };
        let host_socket = runtime.join(path.file_name().context("socket file name")?);
        anyhow::ensure!(
            host_socket.exists(),
            "exposed socket not visible on host at {}",
            host_socket.display()
        );

        // ttyd exposure works the same way (terminal reachability).
        let ttyd_endpoint = client.expose_port(TTYD_PORT).await?;
        anyhow::ensure!(
            matches!(ttyd_endpoint, ExposedEndpoint::Unix { .. }),
            "ttyd exposure must be a unix socket"
        );

        // The bridge is up as soon as the verb returns; the fileserver may
        // still be binding. The real backend proxy retries connects; do the
        // same here before the one-shot streamed upload.
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                if let Ok(mut probe) = tokio::net::UnixStream::connect(&host_socket).await {
                    let request =
                        b"GET /list?path=. HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
                    if probe.write_all(request).await.is_ok() {
                        let mut reply = String::new();
                        if probe.read_to_string(&mut reply).await.is_ok()
                            && reply.starts_with("HTTP/1.1")
                        {
                            break;
                        }
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
        })
        .await
        .context("fileserver never became reachable through the bridge")?;

        // Stream a >100MB multipart upload through the exposed socket with
        // bounded memory (1MB chunks), the browser upload path's wire format.
        let boundary = "oqto-expose-live-boundary";
        let prologue = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; \
             filename=\"big.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        );
        let epilogue = format!("\r\n--{boundary}--\r\n");
        let content_length = prologue.len() + UPLOAD_BYTES + epilogue.len();

        let mut stream = tokio::net::UnixStream::connect(&host_socket)
            .await
            .context("connecting to exposed socket")?;
        let head = format!(
            "POST /file?path=big.bin HTTP/1.1\r\nHost: localhost\r\n\
             Content-Type: multipart/form-data; boundary={boundary}\r\n\
             Content-Length: {content_length}\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(head.as_bytes()).await?;
        stream.write_all(prologue.as_bytes()).await?;
        let chunk = vec![0xA7u8; 1024 * 1024];
        let mut written = 0usize;
        while written < UPLOAD_BYTES {
            let n = chunk.len().min(UPLOAD_BYTES - written);
            stream.write_all(&chunk[..n]).await?;
            written += n;
        }
        stream.write_all(epilogue.as_bytes()).await?;
        stream.flush().await?;

        let mut response = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(120),
            stream.read_to_string(&mut response),
        )
        .await
        .context("reading upload response")??;
        anyhow::ensure!(
            response.starts_with("HTTP/1.1 200"),
            "upload failed: {}",
            &response[..response.len().min(500)]
        );

        // The file must land in the workspace volume with exact size.
        let stat = exec(&placement.runtime_name, &["stat", "-c", "%s", "/workspace/big.bin"])?;
        let size: usize = String::from_utf8_lossy(&stat.stdout).trim().parse()?;
        anyhow::ensure!(
            size == UPLOAD_BYTES,
            "uploaded size {size} != {UPLOAD_BYTES}"
        );

        // Containment: ungranted network stays dead inside the container.
        let net = exec(
            &placement.runtime_name,
            &["curl", "-sf", "--max-time", "5", "http://1.1.1.1"],
        )?;
        anyhow::ensure!(
            !net.status.success(),
            "container reached the network; isolation broken"
        );

        // Session stop tears the bridge down; the host socket disappears.
        client.stop_session("expose-live").await?;
        anyhow::ensure!(
            !host_socket.exists(),
            "exposed socket survived session stop"
        );

        Ok::<_, anyhow::Error>(())
    }
    .await;

    if let Err(error) = &proof {
        let logs = std::process::Command::new("podman")
            .args(["logs", &placement.runtime_name])
            .output()?;
        eprintln!(
            "--- {} logs ---\n{}\nproof failed: {error:#}",
            placement.runtime_name,
            String::from_utf8_lossy(&logs.stderr)
        );
    }
    supervisor.stop(&placement).await?;
    let _ = std::process::Command::new("podman")
        .args(["unshare", "rm", "-rf"])
        .arg(temp.path())
        .status();
    std::mem::forget(temp);
    proof
}
