//! Keep slow local management separate from the primary transport accept loop.
use anyhow::{Result, ensure};
use oqto_runner::{
    client::RunnerClient,
    pair_control::{self, Request},
};
use std::{
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
    time::Duration,
};

#[tokio::test]
async fn slow_owner_client_does_not_block_primary_transport() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let home = temp.path().canonicalize()?;
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
        .env("HOME", &home)
        .env("XDG_RUNTIME_DIR", runtime)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    struct Owned(std::process::Child);
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut owned = Owned(child);
    let control = pair_control::root(&home);
    let mut ready = false;
    for _ in 0..100 {
        ensure!(owned.0.try_wait()?.is_none(), "owned fixture exited");
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
