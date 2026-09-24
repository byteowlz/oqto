//! End-to-end Mac runner spawn proof with a disposable HOME and mock harness.
//! This tests launch/isolation/identity, not Pi RPC or durable chat parity.
#![cfg(target_os = "macos")]

use anyhow::{Context, Result, ensure};
use oqto_runner::{
    client::RunnerClient,
    protocol::{PiCreateSessionRequest, PiSessionConfig},
};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct RunnerChild(Child);
impl Drop for RunnerChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn wait_for_file(path: &Path) -> Result<String> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(path)
                && text.ends_with("DONE\n")
            {
                return Ok::<_, anyhow::Error>(text);
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .context("sandbox payload did not finish its proof")?
}

#[tokio::test]
async fn mac_runner_spawns_generic_and_pi_processes_through_seatbelt() -> Result<()> {
    let root = tempfile::Builder::new()
        .prefix("oq-run-")
        .tempdir_in("/tmp")?;
    let workspace = root.path().join("work");
    let control = root.path().join("control");
    let config_dir = root.path().join("config/oqto");
    for dir in [&workspace, &control, &config_dir] {
        std::fs::create_dir_all(dir)?;
    }
    let socket = control.join("runner.sock");
    let secret = workspace.join("private");
    std::fs::write(&secret, "PRIVATE")?;
    let harness = workspace.join("fixture-pi");
    // No model/provider calls or fake history records. The mock merely proves
    // PiManager supplies RPC argv and inherits the same file policy as spawn.
    std::fs::write(
        &harness,
        "#!/bin/sh\nprintf '%s\\n' \"$$\" > pi-pid\n{ printf '%s\\n' \"$@\"; pwd -P; if cat private; then printf LEAK; else printf DENIED; fi; printf '\\nDONE\\n'; } > pi-proof\nwhile :; do sleep 1; done\n",
    )?;
    std::fs::set_permissions(&harness, std::fs::Permissions::from_mode(0o700))?;
    let config = format!(
        "[local]\nsingle_user = true\nterminal_enabled = false\nworkspace_dir = {:?}\n[pi]\nexecutable = {:?}\n",
        workspace.to_string_lossy(),
        harness.to_string_lossy()
    );
    std::fs::write(config_dir.join("config.toml"), config)?;
    let sandbox = root.path().join("sandbox.toml");
    std::fs::write(
        &sandbox,
        format!(
            "profile = \"minimal\"\nno_new_privs = false\nread_policy = \"allowlist\"\ndeny_read = [{:?}, {:?}]\n",
            control.to_string_lossy(),
            secret.to_string_lossy()
        ),
    )?;
    let log = std::fs::File::create(root.path().join("runner.log"))?;
    let mut runner = RunnerChild(
        Command::new(env!("CARGO_BIN_EXE_oqto-runner"))
            .arg("--socket")
            .arg(&socket)
            .arg("--sandbox-config")
            .arg(&sandbox)
            .env_clear()
            .env("HOME", root.path())
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .env("XDG_CONFIG_HOME", root.path().join("config"))
            .env("XDG_STATE_HOME", root.path().join("state"))
            .env("TMPDIR", root.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()?,
    );
    let result = async {
        let client = RunnerClient::new(socket.clone());
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                ensure!(runner.0.try_wait()?.is_none(), "runner exited before accepting connections");
                if socket.exists() && client.list_sessions().await.is_ok() { return Ok::<_, anyhow::Error>(()); }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }).await.context("runner readiness timeout")??;
        client.spawn_process("generic-proof", "/bin/sh", vec!["-c".into(),
            "{ pwd -P; if cat private; then printf LEAK; else printf DENIED; fi; printf '\\nDONE\\n'; } > generic-proof".into()],
            &workspace, HashMap::new(), true).await?;
        let generic = wait_for_file(&workspace.join("generic-proof")).await?;
        ensure!(generic.ends_with("DENIED\nDONE\n") && !generic.contains("PRIVATE"), "{generic}");
        let session_id = "mac-launch-proof-session";
        let created = client.pi_create_session(PiCreateSessionRequest {
            session_id: session_id.into(), config: PiSessionConfig {
                cwd: workspace.clone(), env: HashMap::from([("AGENT_BROWSER_SOCKET_DIR".into(), workspace.join("sockets").to_string_lossy().into_owned())]),
                ..Default::default()
            },
        }).await?;
        ensure!(created.session_id == session_id, "public routing identity changed");
        let pi = wait_for_file(&workspace.join("pi-proof")).await?;
        ensure!(pi.starts_with("--mode\nrpc\n--approve\n"), "{pi}");
        ensure!(pi.ends_with("DENIED\nDONE\n") && !pi.contains("PRIVATE"), "{pi}");
        ensure!(pi.contains(&workspace.canonicalize()?.to_string_lossy().to_string()), "incorrect cwd: {pi}");
        Ok::<_, anyhow::Error>(())
    }.await;
    // SIGTERM invokes runner shutdown, including the mock Pi process. Drop is
    // the bounded forced-stop backstop if the runner itself fails to exit.
    unsafe {
        libc::kill(runner.0.id() as i32, libc::SIGTERM);
    }
    let stopped = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if runner.0.try_wait()?.is_some() {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    if let Err(error) = &result {
        eprintln!(
            "runner log: {}",
            std::fs::read_to_string(root.path().join("runner.log")).unwrap_or_default()
        );
        eprintln!("{error:#}");
    }
    result?;
    stopped.context("runner shutdown timeout")??;
    let pi_pid: i32 = std::fs::read_to_string(workspace.join("pi-pid"))?
        .trim()
        .parse()?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            // SAFETY: probing a positive PID's existence does not signal it.
            if unsafe { libc::kill(pi_pid, 0) } == -1 {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .context("Pi process survived runner shutdown/revocation")??;
    Ok(())
}
