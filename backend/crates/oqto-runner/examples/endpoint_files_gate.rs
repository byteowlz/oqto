//! Manual live probe for the staged, authenticated runner Files grant.
//! Usage: endpoint_files_gate ENDPOINT_JSON ALLOWED_DIRECTORY DENIED_DIRECTORY [--inventory]
//! The endpoint JSON refers to private client key paths; this command never prints them.
use anyhow::{Result, ensure};
use oqto_runner::client::RunnerClient;
use oqto_runner::transport::RunnerEndpointConfig;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 3 || args.len() == 4 && args[3] == "--inventory",
        "usage: endpoint_files_gate ENDPOINT_JSON ALLOWED_DIRECTORY DENIED_DIRECTORY [--inventory]"
    );
    let endpoint: RunnerEndpointConfig = serde_json::from_slice(&std::fs::read(&args[0])?)?;
    let client = RunnerClient::from_endpoint(&endpoint)?;
    client.ensure_ready_with_recovery().await?;
    let granted = client.list_directory(args[1].as_str(), true).await;
    if args.len() == 4 {
        ensure!(
            granted.is_err(),
            "inventory-only endpoint unexpectedly listed Files"
        );
    } else {
        ensure!(
            granted.is_ok(),
            "granted directory was refused: {granted:?}"
        );
    }
    ensure!(
        client.list_directory(args[2].as_str(), true).await.is_err(),
        "ungranted directory was listed"
    );
    ensure!(
        client.pi_list_sessions().await.is_err(),
        "network Files grant unexpectedly authorized session operations"
    );
    println!("runner network Files grant checks passed");
    Ok(())
}
