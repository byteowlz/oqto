use oqto_placement::{
    JsonPlacementStore, PlacementNetwork, PlacementNetworkMode, PlacementSpec, PlacementStore,
    PlacementSupervisor, PodmanSupervisor, RunnerServerTlsConfig,
};
use oqto_runner::transport::RunnerEndpointConfig;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let image = std::env::var("OQTO_WORKSPACE_IMAGE")?;
    let workspace_id = std::env::var("OQTO_WORKSPACE_ID")?;
    let root = PathBuf::from(std::env::var("OQTO_PLACEMENT_ROOT")?);
    let store_path = PathBuf::from(std::env::var("OQTO_PLACEMENT_STORE")?);
    let runtime = root.join("runtime");
    let workspace_dir = std::env::var("OQTO_WORKSPACE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("workspace"));
    tokio::fs::create_dir_all(&runtime).await?;

    let (runner_endpoint, server_tls) = if let Ok(address) = std::env::var("OQTO_LISTEN_ADDRESS") {
        let tls_root = PathBuf::from(std::env::var("OQTO_TLS_ROOT")?);
        (
            RunnerEndpointConfig::TcpTls {
                address: address.parse()?,
                server_name: std::env::var("OQTO_TLS_SERVER_NAME")?,
                ca: tls_root.join("ca.pem"),
                certificate: tls_root.join("client.pem"),
                key: tls_root.join("client.key"),
            },
            Some(RunnerServerTlsConfig {
                client_ca: tls_root.join("ca.pem"),
                certificate: tls_root.join("server.pem"),
                key: tls_root.join("server.key"),
            }),
        )
    } else {
        (
            RunnerEndpointConfig::Unix {
                path: runtime.join("runner.sock"),
            },
            None,
        )
    };

    let limits_enabled = std::env::var_os("OQTO_DISABLE_LIMITS").is_none();
    let spec = PlacementSpec {
        workspace_id,
        account_id: "oqto-e2e".to_string(),
        image,
        workspace_dir,
        state_dir: root.join("state"),
        runner_endpoint,
        server_tls,
        environment: BTreeMap::new(),
        cpu_limit: limits_enabled.then(|| "2".to_string()),
        memory_limit: limits_enabled.then(|| "2g".to_string()),
        network: PlacementNetwork {
            mode: PlacementNetworkMode::Open,
            endpoints: Vec::new(),
        },
    };
    let supervisor = PodmanSupervisor::new();
    let placement = supervisor.start(&spec).await?;
    let store = JsonPlacementStore::open(store_path).await?;
    store.put(placement.clone()).await?;
    println!("{}", serde_json::to_string(&placement)?);

    tokio::signal::ctrl_c().await?;
    store.remove(&placement.id).await?;
    supervisor.stop(&placement).await
}
