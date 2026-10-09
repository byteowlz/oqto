#![cfg(target_os = "linux")]

use oqto_placement::{
    PlacementNetwork, PlacementNetworkMode, PlacementSpec, PlacementSupervisor, PodmanSupervisor,
    RunnerServerTlsConfig,
};
use oqto_runner::client::RunnerClient;
use oqto_runner::transport::RunnerEndpointConfig;
use std::collections::BTreeMap;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::Command;

struct Pki {
    ca: PathBuf,
    server_cert: PathBuf,
    server_key: PathBuf,
    client_cert: PathBuf,
    client_key: PathBuf,
}

fn openssl(cwd: &Path, args: &[&str]) -> anyhow::Result<()> {
    let output = Command::new("openssl")
        .current_dir(cwd)
        .args(args)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "openssl failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn create_pki(root: &Path, name: &str) -> anyhow::Result<Pki> {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir)?;
    openssl(
        &dir,
        &[
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=Oqto Test CA",
            "-keyout",
            "ca.key",
            "-out",
            "ca.pem",
        ],
    )?;
    std::fs::write(
        dir.join("server.ext"),
        "subjectAltName=DNS:runner.oqto.test\nextendedKeyUsage=serverAuth\n",
    )?;
    openssl(
        &dir,
        &[
            "req",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-subj",
            "/CN=runner.oqto.test",
            "-keyout",
            "server.key",
            "-out",
            "server.csr",
        ],
    )?;
    openssl(
        &dir,
        &[
            "x509",
            "-req",
            "-days",
            "1",
            "-in",
            "server.csr",
            "-CA",
            "ca.pem",
            "-CAkey",
            "ca.key",
            "-CAcreateserial",
            "-extfile",
            "server.ext",
            "-out",
            "server.pem",
        ],
    )?;
    std::fs::write(dir.join("client.ext"), "extendedKeyUsage=clientAuth\n")?;
    openssl(
        &dir,
        &[
            "req",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-subj",
            "/CN=oqto-control-plane",
            "-keyout",
            "client.key",
            "-out",
            "client.csr",
        ],
    )?;
    openssl(
        &dir,
        &[
            "x509",
            "-req",
            "-days",
            "1",
            "-in",
            "client.csr",
            "-CA",
            "ca.pem",
            "-CAkey",
            "ca.key",
            "-CAcreateserial",
            "-extfile",
            "client.ext",
            "-out",
            "client.pem",
        ],
    )?;
    Ok(Pki {
        ca: dir.join("ca.pem"),
        server_cert: dir.join("server.pem"),
        server_key: dir.join("server.key"),
        client_cert: dir.join("client.pem"),
        client_key: dir.join("client.key"),
    })
}

fn free_address() -> anyhow::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?)
}

fn spec(
    root: &Path,
    image: &str,
    workspace_id: &str,
    address: SocketAddr,
    pki: &Pki,
) -> PlacementSpec {
    PlacementSpec {
        workspace_id: workspace_id.to_string(),
        account_id: "placement-tls-live-test".to_string(),
        image: image.to_string(),
        workspace_dir: root.join("workspace"),
        state_dir: root.join("state"),
        runner_endpoint: RunnerEndpointConfig::TcpTls {
            address,
            server_name: "runner.oqto.test".to_string(),
            ca: pki.ca.clone(),
            certificate: pki.client_cert.clone(),
            key: pki.client_key.clone(),
        },
        server_tls: Some(RunnerServerTlsConfig {
            client_ca: pki.ca.clone(),
            certificate: pki.server_cert.clone(),
            key: pki.server_key.clone(),
        }),
        environment: BTreeMap::new(),
        cpu_limit: Some("1".to_string()),
        memory_limit: Some("1g".to_string()),
        userns: Default::default(),
        network: PlacementNetwork {
            mode: PlacementNetworkMode::Open,
            endpoints: Vec::new(),
        },
    }
}

async fn wait_ready(endpoint: &RunnerEndpointConfig) -> anyhow::Result<()> {
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            if let Ok(client) = RunnerClient::from_endpoint(endpoint)
                && client.ensure_ready_with_recovery().await.is_ok()
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    })
    .await?;
    Ok(())
}

/// Proves mTLS placement, untrusted-client rejection, CA rotation/revocation,
/// restart, and reconnect through the real rootless Podman adapter.
#[tokio::test]
#[ignore = "requires rootless Podman, OpenSSL, and a built Workspace image"]
async fn rootless_podman_mtls_rotation_and_revocation() -> anyhow::Result<()> {
    let image = std::env::var("OQTO_WORKSPACE_IMAGE")?;
    let temp = tempfile::tempdir()?;
    let address = free_address()?;
    let first_pki = create_pki(temp.path(), "first")?;
    let first_spec = spec(temp.path(), &image, "tls-live", address, &first_pki);
    let supervisor = PodmanSupervisor::new();

    let first = supervisor.start(&first_spec).await?;
    wait_ready(&first.runner_endpoint).await?;

    let rogue_pki = create_pki(temp.path(), "rogue")?;
    let rogue_endpoint = RunnerEndpointConfig::TcpTls {
        address,
        server_name: "runner.oqto.test".to_string(),
        ca: first_pki.ca.clone(),
        certificate: rogue_pki.client_cert,
        key: rogue_pki.client_key,
    };
    let rogue = RunnerClient::from_endpoint(&rogue_endpoint)?;
    anyhow::ensure!(rogue.ensure_ready_with_recovery().await.is_err());
    supervisor.stop(&first).await?;

    let second_pki = create_pki(temp.path(), "second")?;
    let second_spec = spec(temp.path(), &image, "tls-live", address, &second_pki);
    let second = supervisor.start(&second_spec).await?;
    wait_ready(&second.runner_endpoint).await?;

    let revoked = RunnerClient::from_endpoint(&first_spec.runner_endpoint)?;
    anyhow::ensure!(revoked.ensure_ready_with_recovery().await.is_err());
    supervisor.stop(&second).await?;
    Ok(())
}
