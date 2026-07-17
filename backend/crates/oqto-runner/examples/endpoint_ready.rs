use anyhow::Result;
use oqto_runner::client::RunnerClient;
use oqto_runner::transport::RunnerEndpointConfig;

#[tokio::main]
async fn main() -> Result<()> {
    let path = std::env::args().nth(1).ok_or_else(|| anyhow::anyhow!("endpoint JSON path required"))?;
    let endpoint: RunnerEndpointConfig = serde_json::from_slice(&std::fs::read(path)?)?;
    let client = RunnerClient::from_endpoint(&endpoint)?;
    client.ensure_ready_with_recovery().await?;
    println!("ready {}", client.endpoint_description());
    Ok(())
}
