use super::*;
use sha2::{Digest, Sha256};
use std::os::unix::fs::MetadataExt;

#[tokio::test]
async fn stopped_saved_listener_repairs_without_ticket_or_runner_restart() -> Result<()> {
    let temp = tempfile::Builder::new()
        .prefix("oqrecover-")
        .tempdir_in("/tmp")?;
    let home = temp.path().canonicalize()?;
    std::fs::create_dir(home.join("work"))?;
    std::fs::create_dir(home.join("runtime"))?;
    std::fs::set_permissions(home.join("runtime"), std::fs::Permissions::from_mode(0o700))?;
    let mut first = Host::start(&home)?;
    first.ready().await?;
    let control = pair_control::root(&home);
    let selected = plan(&home)?;
    let initial = pair_control::request(
        &control,
        &Request::Enable {
            plan: selected.clone(),
        },
    )
    .await?;
    first.stop()?;
    let db = initial.state.join("personal.sqlite3");
    let marker = initial.state.join("store-owner.json");
    let meta = db.metadata()?;
    let legacy = format!(
        "{{\"purpose\":\"oqto.personal-store.v1\",\"dev\":{},\"ino\":{}}}\n",
        meta.dev() ^ 1,
        meta.ino()
    );
    std::fs::write(&marker, &legacy)?;
    let mut second = Host::start(&home)?;
    second.ready().await?;
    let pid = second.child.id();
    let stopped = pair_control::request(&control, &Request::Status).await?;
    ensure!(!stopped.active && stopped.plan == Some(selected.clone()));
    ensure!(
        std::fs::read_to_string(home.join("fixture-stderr.log"))?
            .contains("legacy store mount identity changed")
    );
    std::fs::create_dir(home.join("other"))?;
    let mut foreign = selected.clone();
    foreign.workspaces.push(home.join("other"));
    ensure!(
        pair_control::request(&control, &Request::Enable { plan: foreign })
            .await
            .is_err()
    );
    let bytes = std::fs::read(&db)?;
    let digest = format!("{:x}", Sha256::digest(legacy.as_bytes()));
    let repaired = Command::new(env!("CARGO_BIN_EXE_oqto-runner"))
        .args([
            "pair",
            "--repair-owner-marker",
            &digest,
            "--output",
            "json",
            "--runner-home",
        ])
        .arg(&home)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .output()?;
    ensure!(
        repaired.status.success(),
        "repair failed: {}",
        String::from_utf8_lossy(&repaired.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&repaired.stdout)?;
    ensure!(receipt["ticket_issued"] == false && receipt["repaired"] == true);
    ensure!(std::fs::read(&db)? == bytes && db.metadata()?.ino() == meta.ino());
    let recovered = Command::new(env!("CARGO_BIN_EXE_oqto-runner"))
        .args(["pair", "--recover-listener", "--runner-home"])
        .arg(&home)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .output()?;
    ensure!(
        recovered.status.success(),
        "recover failed: {}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    let restored = pair_control::request(&control, &Request::Status).await?;
    ensure!(restored.active && restored.pid == pid && restored.plan == Some(selected));
    second.stop()?;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&db)
                .read_only(true),
        )
        .await?;
    let tickets: i64 = sqlx::query_scalar("SELECT count(*) FROM personal_pairing_tickets")
        .fetch_one(&pool)
        .await?;
    ensure!(tickets == 0, "repair/recovery minted a ticket");
    pool.close().await;
    Ok(())
}

#[test]
fn repair_and_recovery_are_explicit_non_pairing_operations() -> Result<()> {
    ensure!(
        !parse(&["--recover-listener"])?
            .mode(false)
            .map_err(anyhow::Error::msg)?
    );
    ensure!(
        !parse(&["--repair-owner-marker", &"a".repeat(64)])?
            .mode(false)
            .map_err(anyhow::Error::msg)?
    );
    ensure!(parse(&["--recover-listener", "--access", "full"]).is_err());
    ensure!(
        parse(&[
            "--repair-owner-marker",
            &"a".repeat(64),
            "--recover-listener"
        ])
        .is_err()
    );
    Ok(())
}
