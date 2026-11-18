//! Keep slow local management separate from the primary transport accept loop.
use anyhow::{Result, ensure};
use oqto_runner::{
    client::RunnerClient,
    pair_control::{self, Request},
};
use std::{
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_fixture(home: &Path) -> Result<Owned> {
    pair_control::validate_socket_path(&pair_control::root(home).join("control.sock"))?;
    let log = std::fs::File::create(home.join("runner.log"))?;
    let config = home.join("config.toml");
    std::fs::write(
        &config,
        "[local]\nsingle_user=true\n[local.linux_users]\nenabled=false\n[pi]\nexecutable='pi-must-not-launch'\n",
    )?;
    let runtime = home.join("runtime");
    std::fs::create_dir(&runtime)?;
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700))?;
    let socket = home.join("primary.sock");
    let child = Command::new(env!("CARGO_BIN_EXE_oqto-runner"))
        .arg("--no-sandbox")
        .arg("--config")
        .arg(config)
        .arg("--socket")
        .arg(&socket)
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("HOME", home)
        .env("USER", "synthetic-fixture")
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_RUNTIME_DIR", runtime)
        .env("TMPDIR", home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()?;
    Ok(Owned(child))
}

#[tokio::test]
async fn slow_owner_client_does_not_block_primary_transport() -> Result<()> {
    // Darwin's default TMPDIR plus pairing/control.sock can exceed sun_path.
    let temp = tempfile::Builder::new()
        .prefix("oqtr-")
        .tempdir_in("/tmp")?;
    let home = temp.path().canonicalize()?;
    let control = pair_control::root(&home);
    let socket = home.join("primary.sock");
    let log_path = home.join("runner.log");
    let mut owned = spawn_fixture(&home)?;
    let mut ready = false;
    for _ in 0..100 {
        ensure!(
            owned.0.try_wait()?.is_none(),
            "owned fixture exited: {}",
            std::fs::read_to_string(&log_path).unwrap_or_default()
        );
        if pair_control::request(&control, &Request::Status)
            .await
            .is_ok()
        {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    ensure!(ready, "fixture readiness deadline");
    let _slow = tokio::net::UnixStream::connect(control.join("control.sock")).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    tokio::time::timeout(
        Duration::from_secs(1),
        RunnerClient::new(socket).get_capabilities(),
    )
    .await??;
    Ok(())
}
