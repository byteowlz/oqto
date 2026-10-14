//! Staged mTLS personal Pi admission proof. Use a disposable runner HOME.
//! endpoint_personal_pi ENDPOINT_JSON WORK_DIRECTORY [--denied | OUTSIDE_FIXTURE]
//! Never point OUTSIDE_FIXTURE at a real credential or user document.
use anyhow::{Result, ensure};
use oqto_runner::client::RunnerClient;
use oqto_runner::protocol::{PiCreateSessionRequest, PiSessionConfig};
use oqto_runner::transport::RunnerEndpointConfig;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 3,
        "usage: endpoint_personal_pi ENDPOINT_JSON WORK_DIRECTORY [--denied | OUTSIDE_FIXTURE]"
    );
    let endpoint: RunnerEndpointConfig = serde_json::from_slice(&std::fs::read(&args[0])?)?;
    let client = RunnerClient::from_endpoint(&endpoint)?;
    client.ensure_ready_with_recovery().await?;
    let id = format!(
        "personal-proof-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
    );
    let request = PiCreateSessionRequest {
        session_id: id,
        config: PiSessionConfig {
            cwd: PathBuf::from(&args[1]),
            ..Default::default()
        },
    };
    if args[2] == "--denied" {
        ensure!(
            client.pi_create_session(request).await.is_err(),
            "narrow Files grant unexpectedly admitted Pi"
        );
        ensure!(
            client.pi_list_sessions().await.is_err(),
            "narrow Files grant unexpectedly listed Pi sessions"
        );
        println!("narrow-root network Pi admission denied");
        return Ok(());
    }
    let mut invalid = request.clone();
    invalid.config.cwd = PathBuf::from("../untrusted");
    ensure!(
        client.pi_create_session(invalid).await.is_err(),
        "relative Pi work directory was admitted"
    );
    let created = client.pi_create_session(request).await?;
    let result = async {
        let state = client.pi_get_state(&created.session_id).await?;
        ensure!(
            !state.session_id.is_empty(),
            "missing runner-owned Pi session identity"
        );
        // The full-principal flag intentionally allows a harmless fixture
        // outside cwd. This must never be mistaken for narrow confinement.
        let fixture = args[2].replace('\'', "'\\''");
        let command = format!("/bin/cat -- '{fixture}'");
        let output = client.pi_bash(&created.session_id, &command).await?;
        ensure!(
            output.exit_code == 0 && output.output.contains("personal-proof-fixture"),
            "PiBash failed to read the fixture"
        );
        Ok::<(), anyhow::Error>(())
    }
    .await;
    let close = client.pi_close_session(&created.session_id).await;
    result?;
    close?;
    println!("personal Pi session and isolated fixture PiBash passed");
    Ok(())
}
