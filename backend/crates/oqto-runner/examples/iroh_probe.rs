use anyhow::{Context, Result};
use iroh::endpoint::presets::N0;
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey};
use oqto_runner::iroh_transport::{IrohRunnerConnector, IrohRunnerListener, OQTO_RUNNER_ALPN};
use oqto_runner::transport::{RunnerConnector, RunnerListener};
use oqto_runner::wire::{read_json_frame, write_json_frame};
use std::collections::HashSet;
use std::path::Path;
use std::str::FromStr;
use tokio::io::BufReader;

fn read_key(path: &Path) -> Result<SecretKey> {
    serde_json::from_slice(&std::fs::read(path)?).context("parsing Iroh secret key")
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    match args.get(1).map(String::as_str) {
        Some("keygen") => {
            let path = Path::new(args.get(2).context("keygen PATH")?);
            let key = SecretKey::generate();
            std::fs::write(path, serde_json::to_vec(&key)?)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
            }
            println!("{}", key.public());
        }
        Some("server") => {
            let key = read_key(Path::new(args.get(2).context("server KEY")?))?;
            let allowed = EndpointId::from_str(args.get(3).context("server KEY ALLOWED_ID")?)?;
            let count: usize = args.get(4).map_or(Ok(2), |value| value.parse())?;
            let endpoint = Endpoint::builder(N0)
                .secret_key(key)
                .alpns(vec![OQTO_RUNNER_ALPN.to_vec()])
                .bind()
                .await?;
            let listener = IrohRunnerListener::new(endpoint.clone(), HashSet::from([allowed]))?;
            endpoint.online().await;
            println!("ADDR {}", serde_json::to_string(&endpoint.addr())?);
            for index in 0..count {
                let stream = listener.accept().await?;
                let mut stream = BufReader::new(stream);
                let sent: String = read_json_frame(&mut stream)
                    .await?
                    .context("client closed before probe")?;
                write_json_frame(stream.get_mut(), &format!("ack:{index}:{sent}")).await?;
            }
            // Let the bridge flush the final framed response before closing QUIC.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            endpoint.close().await;
        }
        Some("client") => {
            let key = read_key(Path::new(args.get(2).context("client KEY")?))?;
            let remote: EndpointAddr = serde_json::from_slice(&std::fs::read(
                args.get(3).context("client KEY ADDR_JSON")?,
            )?)?;
            let count: usize = args.get(4).map_or(Ok(2), |value| value.parse())?;
            let endpoint = Endpoint::builder(N0).secret_key(key).bind().await?;
            let connector = IrohRunnerConnector::new(endpoint.clone(), remote);
            for index in 0..count {
                let started = std::time::Instant::now();
                let mut stream = connector.connect().await?;
                write_json_frame(&mut stream, &format!("ping:{index}")).await?;
                let mut stream = BufReader::new(stream);
                let response: String = read_json_frame(&mut stream)
                    .await?
                    .context("server closed before response")?;
                println!("{} {}ms", response, started.elapsed().as_millis());
            }
            endpoint.close().await;
        }
        _ => anyhow::bail!(
            "usage: iroh_probe keygen PATH | server KEY ALLOWED_ID [COUNT] | client KEY ADDR_JSON [COUNT]"
        ),
    }
    Ok(())
}
