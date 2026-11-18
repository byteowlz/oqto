#![cfg(unix)]
use anyhow::{Context, Result, ensure};
use oqto_posts::{
    server::{Registration, Role},
    service::{ServiceConfig, add_identity},
    wire::{Route, Stamp},
};
use tokio::io::{AsyncBufReadExt, BufReader};

#[test]
fn config_unknowns_fail_closed_and_schema_stays_current() -> Result<()> {
    let example = include_str!("../examples/config.toml");
    let config: ServiceConfig = toml::from_str(example)?;
    config.validate()?;
    ensure!(!config.enabled, "must be opt-in");
    for broken in [
        example.replace("enabled = false", "enabled = false\nunknown = 1"),
        example.replace("role = \"agent\"", "role = \"admin\""),
        example.replace("route = \"local\"", "route = \"unknown\""),
        example.replace("max_connections = 64", "max_connections = 0"),
    ] {
        ensure!(
            toml::from_str::<ServiceConfig>(&broken).map_or(true, |c| c.validate().is_err()),
            "bad config accepted"
        );
    }
    for session in ["pending-123", "tmp:123"] {
        let mut invalid = config.clone();
        invalid.identities[0].stamp.session = Some(session.into());
        ensure!(invalid.validate().is_err(), "temporary Session accepted");
    }
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../examples/config.schema.json"))?;
    ensure!(
        schema == serde_json::from_str::<serde_json::Value>(&ServiceConfig::schema()?)?,
        "schema stale"
    );
    Ok(())
}

#[test]
fn owner_registry_writes_new_candidate_only() -> Result<()> {
    let config: ServiceConfig = toml::from_str(include_str!("../examples/config.toml"))?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("new.toml");
    let identity = Registration {
        name: "builder".into(),
        role: Role::Agent,
        stamp: Stamp {
            agent: "builder".into(),
            workspace: "work".into(),
            session: None,
            task: None,
            route: Route::Private,
        },
    };
    add_identity(&config, identity.clone(), &path)?;
    ensure!(
        ServiceConfig::load(&path)?.identities.len() == 3,
        "not added"
    );
    let before = std::fs::read(&path)?;
    ensure!(
        add_identity(&config, identity.clone(), &path).is_err(),
        "overwrote config"
    );
    ensure!(std::fs::read(&path)? == before, "changed file on failure");
    let duplicate = dir.path().join("duplicate.toml");
    let mut config = config;
    config.identities.push(identity.clone());
    ensure!(
        add_identity(&config, identity, &duplicate).is_err() && !duplicate.exists(),
        "duplicate registered"
    );
    Ok(())
}

#[tokio::test]
async fn foreground_service_starts_and_sigterm_cleans_up() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let config_path = dir.path().join("config.toml");
    let mut config: ServiceConfig = toml::from_str(include_str!("../examples/config.toml"))?;
    config.enabled = true;
    config.database = dir.path().join("private-state/posts.db");
    config.socket_directory = dir.path().join("s");
    std::fs::write(&config_path, toml::to_string(&config)?)?;
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_oqto-posts"))
        .env_clear()
        .arg("--config")
        .arg(&config_path)
        .args(["service", "run"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let mut ready = String::new();
    let stdout = child.stdout.take().context("child stdout")?;
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        BufReader::new(stdout).read_line(&mut ready),
    )
    .await??;
    if !ready.contains("\"ready\":true") {
        let output = child.wait_with_output().await?;
        anyhow::bail!(
            "not ready: {ready}; {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    ensure!(
        config.socket_directory.join("reviewer.sock").exists(),
        "no socket"
    );
    let pid = child.id().context("no child pid")?;
    ensure!(
        tokio::process::Command::new("/bin/kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .await?
            .success(),
        "could not terminate test child"
    );
    ensure!(
        tokio::time::timeout(std::time::Duration::from_secs(5), child.wait())
            .await??
            .success(),
        "unclean exit"
    );
    ensure!(
        !config.socket_directory.join("reviewer.sock").exists(),
        "socket left behind"
    );
    config.enabled = false;
    std::fs::write(&config_path, toml::to_string(&config)?)?;
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_oqto-posts"))
        .arg("--config")
        .arg(&config_path)
        .args(["service", "run"])
        .output()
        .await?;
    ensure!(
        !output.status.success() && String::from_utf8_lossy(&output.stderr).contains("disabled"),
        "off by default failed"
    );
    Ok(())
}
