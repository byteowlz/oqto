use super::*;
use anyhow::Context;

#[tokio::test]
async fn owner_outputs_copyable_complete_payload_and_human_local_expiry() -> Result<()> {
    let temp = tempfile::Builder::new()
        .prefix("oqoutput-")
        .tempdir_in("/tmp")?;
    let home = temp.path().canonicalize()?;
    std::fs::create_dir(home.join("work"))?;
    std::fs::create_dir(home.join("runtime"))?;
    std::fs::set_permissions(home.join("runtime"), std::fs::Permissions::from_mode(0o700))?;
    let mut host = Host::start(&home)?;
    host.ready().await?;
    let selected = plan(&home)?;
    let active = pair_control::request(
        &pair_control::root(&home),
        &Request::Enable { plan: selected },
    )
    .await?;
    let invoke = |output: &str| {
        Command::new(env!("CARGO_BIN_EXE_oqto-runner"))
            .args(["personal", "pair", "--state"])
            .arg(&active.state)
            .args(["--output", output])
            .env("HOME", &home)
            .env("TZ", "UTC")
            .output()
    };
    let terminal = invoke("terminal")?;
    ensure!(terminal.status.success());
    let human = String::from_utf8(terminal.stdout)?;
    ensure!(
        !human.contains("Unix"),
        "expiry still uses raw Unix seconds"
    );
    ensure!(
        human.contains("Expires:") && human.contains("+00:00") && human.contains("remaining"),
        "missing local date/time, offset or countdown"
    );
    let code = invoke("code")?;
    ensure!(code.status.success(), "code output unsupported");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    let parsed = oqto_personal_wire::PairingQr::parse(&code.stdout, now)?;
    ensure!(parsed.to_json()? == code.stdout.strip_suffix(b"\n").unwrap_or(&code.stdout));
    let hints = String::from_utf8(code.stderr)?;
    ensure!(hints.contains("Expires:") && hints.contains("+00:00"));
    let expiry = chrono::DateTime::from_timestamp(parsed.expires_at(), 0)
        .context("invalid fixture expiry")?;
    ensure!(
        hints.contains(&expiry.format("%Y-%m-%d %H:%M:%S %:z").to_string()),
        "human expiry differs from the exact payload instant"
    );
    let json = invoke("json")?;
    ensure!(json.status.success() && json.stderr.is_empty());
    oqto_personal_wire::PairingQr::parse(&json.stdout, now)?;
    host.stop()?;
    Ok(())
}
