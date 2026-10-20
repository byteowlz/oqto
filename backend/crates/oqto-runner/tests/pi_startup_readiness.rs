#![cfg(unix)]
//! Actual daemon RPC regression; private HOME/XDG, synthetic Pi, no credentials.
use anyhow::{Context, Result};
use oqto_runner::client::RunnerClient;
use oqto_runner::protocol::{PiCreateSessionRequest, PiSessionConfig};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct Fixture {
    home: tempfile::TempDir,
    child: Child,
    client: RunnerClient,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Fixture {
    async fn start(mode: &str) -> Result<Self> {
        let home = tempfile::tempdir()?;
        let pi = home.path().join("pi");
        std::fs::write(
            &pi,
            r##"#!/usr/bin/python3
import json,os,sys,time
from pathlib import Path
home=Path(os.environ['HOME'])
state={'model':None,'thinkingLevel':'off','isStreaming':False,'isCompacting':False,
 'steeringMode':'all','followUpMode':'all','sessionId':'synthetic-native-fixture',
 'autoCompactionEnabled':True,'messageCount':0,'pendingMessageCount':0}
for line in sys.stdin:
 request=json.loads(line)
 probe=request.get('id')=='oqto_startup_probe'
 mode=(home/'mode').read_text().strip()
 if probe and mode=='rejected-probe':
  print('Bearer synthetic-secret-DO-NOT-LOG',file=sys.stderr,flush=True);sys.exit(1)
 if not probe:
  with (home/'manager-pids').open('a') as f:f.write(str(os.getpid())+'\n')
  if mode=='early-exit':
   print('Bearer synthetic-secret-DO-NOT-LOG',file=sys.stderr,flush=True)
   print('Tool "Todo" conflicts between /fixture/pi-todolist/index.ts and /fixture/pi-oqto-todos/index.ts',file=sys.stderr,flush=True)
   sys.exit(1)
  if mode=='slow-ready':time.sleep(.5)
  if mode=='hang-manager':time.sleep(30);sys.exit(0)
  if request.get('type')!='get_state':sys.exit(1)
 print(json.dumps({'type':'response','id':request.get('id'),'success':True,'data':state}),flush=True)
 if not probe and mode=='broken-stdin':
  os.close(0);time.sleep(30);sys.exit(0)
"##,
        )?;
        std::fs::set_permissions(&pi, std::fs::Permissions::from_mode(0o700))?;
        std::fs::write(home.path().join("mode"), mode)?;
        let config = home.path().join("config.toml");
        std::fs::write(
            &config,
            format!(
                "[local]\nsingle_user=true\nworkspace_dir={}\n[pi]\nexecutable={}\n",
                serde_json::to_string(home.path())?,
                serde_json::to_string(&pi)?
            ),
        )?;
        let socket = home.path().join("runner.sock");
        let mut command = Command::new(env!("CARGO_BIN_EXE_oqto-runner"));
        command
            .args(["--no-sandbox", "--socket"])
            .arg(&socket)
            .arg("--config")
            .arg(config)
            .env_clear()
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .env("HOME", home.path())
            .env("USER", "synthetic-fixture")
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("XDG_DATA_HOME", home.path().join("data"))
            .env("XDG_STATE_HOME", home.path().join("state"))
            .env("XDG_CACHE_HOME", home.path().join("cache"))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if mode == "rejected-probe" {
            command
                .args(["check-pi", "--workdir"])
                .arg(home.path())
                .stdout(Stdio::piped());
        }
        let child = command.spawn()?;
        let client = RunnerClient::new(socket);
        let mut fixture = Self {
            home,
            child,
            client,
        };
        if mode != "rejected-probe" {
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if fixture.client.agent_list_sessions().await.is_ok() {
                        return Ok::<_, anyhow::Error>(());
                    }
                    anyhow::ensure!(
                        fixture.child.try_wait()?.is_none(),
                        "fixture daemon exited before readiness"
                    );
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await??;
        }
        Ok(fixture)
    }

    fn request(&self, session_id: &str) -> PiCreateSessionRequest {
        PiCreateSessionRequest {
            session_id: session_id.to_string(),
            config: PiSessionConfig {
                cwd: self.home.path().to_path_buf(),
                ..Default::default()
            },
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn early_exit_returns_bounded_diagnostic_and_never_leaves_an_idle_slot() -> Result<()> {
    let fixture = Fixture::start("early-exit").await?;
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        fixture
            .client
            .pi_create_session(fixture.request("oqto-startup-failed")),
    )
    .await?;
    let error = result.err().context("dead Pi was reported created")?;
    let diagnostic = format!("{error:#}");
    assert!(
        diagnostic.contains("exited") || diagnostic.contains("pipe closed"),
        "{diagnostic}"
    );
    assert!(!diagnostic.contains("synthetic-secret-DO-NOT-LOG"));
    assert!(diagnostic.contains("Todo"), "{diagnostic}");
    assert!(
        diagnostic.contains("pi-oqto-todos/index.ts"),
        "{diagnostic}"
    );
    assert!(diagnostic.contains("euid="), "{diagnostic}");
    assert!(fixture.client.agent_list_sessions().await?.is_empty());
    // Same routing ID is reusable only after confirmed removal, never a stuck
    // creating bit or false success. No input is retried or native IDs rewritten.
    std::fs::write(fixture.home.path().join("mode"), "healthy")?;
    fixture
        .client
        .pi_create_session(fixture.request("oqto-startup-failed"))
        .await?;
    fixture
        .client
        .agent_get_state("oqto-startup-failed")
        .await?;
    fixture
        .client
        .agent_close_session("oqto-startup-failed")
        .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_create_waits_for_real_state_and_spawns_once() -> Result<()> {
    let fixture = Fixture::start("slow-ready").await?;
    let first = fixture
        .client
        .pi_create_session(fixture.request("oqto-startup-concurrent"));
    let second = fixture
        .client
        .pi_create_session(fixture.request("oqto-startup-concurrent"));
    let (first, second) = tokio::join!(first, second);
    first?;
    second?;
    let pids = std::fs::read_to_string(fixture.home.path().join("manager-pids"))?;
    assert_eq!(
        pids.lines().collect::<std::collections::HashSet<_>>().len(),
        1
    );
    fixture
        .client
        .agent_get_state("oqto-startup-concurrent")
        .await?;
    fixture
        .client
        .agent_close_session("oqto-startup-concurrent")
        .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn broken_stdin_evicts_even_when_stdout_and_child_remain_alive() -> Result<()> {
    let fixture = Fixture::start("broken-stdin").await?;
    fixture
        .client
        .pi_create_session(fixture.request("oqto-broken-stdin"))
        .await?;
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        fixture.client.agent_get_state("oqto-broken-stdin"),
    )
    .await?;
    assert!(result.is_err(), "closed stdin was reported ready");
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let sessions = fixture.client.agent_list_sessions().await?;
            if sessions.is_empty() {
                return Ok::<_, anyhow::Error>(());
            }
            assert!(
                sessions
                    .iter()
                    .all(|session| session.state != oqto_runner::protocol::PiSessionState::Idle)
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_creation_reaps_owned_child_and_releases_creation_permit() -> Result<()> {
    let mut fixture = Fixture::start("healthy").await?;
    fixture.child.kill()?;
    fixture.child.wait()?;
    std::fs::write(fixture.home.path().join("mode"), "hang-manager")?;
    let status = Command::new(std::env::current_exe()?)
        .args(["--exact", "cancel_create_in_private_process"])
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("HOME", fixture.home.path())
        .env("XDG_DATA_HOME", fixture.home.path().join("data"))
        .env("XDG_CONFIG_HOME", fixture.home.path().join("config"))
        .env("OQTO_STARTUP_CANCELLATION_FIXTURE", fixture.home.path())
        .status()?;
    anyhow::ensure!(status.success(), "isolated cancellation subprocess failed");
    assert_eq!(
        std::fs::read_to_string(fixture.home.path().join("cancellation-proof"))?,
        "reaped-and-reused"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_create_in_private_process() -> Result<()> {
    let Some(home) = std::env::var_os("OQTO_STARTUP_CANCELLATION_FIXTURE") else {
        return Ok(());
    };
    let home = std::path::PathBuf::from(home);
    anyhow::ensure!(
        std::env::var_os("HOME").as_deref() == Some(home.as_os_str()),
        "private HOME mismatch"
    );
    let manager =
        oqto_runner::pi_manager::PiSessionManager::new(oqto_runner::pi_manager::PiManagerConfig {
            pi_binary: home.join("pi"),
            default_cwd: home.clone(),
            sandbox_config: None,
            ..Default::default()
        });
    let owned = manager.clone();
    let config = oqto_runner::pi_manager::PiSessionConfig {
        cwd: home.clone(),
        ..Default::default()
    };
    let request = config.clone();
    let task = tokio::spawn(async move {
        owned
            .get_or_create_session("oqto-cancel-fixture", request)
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        while !home.join("manager-pids").is_file() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    let pid = std::fs::read_to_string(home.join("manager-pids"))?
        .lines()
        .last()
        .context("fixture PID missing")?
        .to_string();
    task.abort();
    assert!(task.await.err().is_some_and(|error| error.is_cancelled()));
    tokio::time::timeout(Duration::from_secs(3), async {
        while !manager.list_sessions().await.is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let alive = Command::new("kill")
                .args(["-0", &pid])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()?;
            if !alive.success() {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("cancelled owned child still running")??;
    std::fs::write(home.join("mode"), "healthy")?;
    tokio::time::timeout(
        Duration::from_secs(3),
        manager.get_or_create_session("oqto-cancel-fixture", config),
    )
    .await??;
    manager.close_session("oqto-cancel-fixture").await?;
    std::fs::write(home.join("cancellation-proof"), "reaped-and-reused")?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_explicit_native_preflight_never_reports_ready_or_falls_back() -> Result<()> {
    let mut fixture = Fixture::start("rejected-probe").await?;
    tokio::time::timeout(Duration::from_secs(7), async {
        loop {
            if let Some(status) = fixture.child.try_wait()? {
                anyhow::ensure!(!status.success(), "rejected Pi was reported ready");
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await??;
    assert!(!Path::new(&fixture.home.path().join("runner.sock")).exists());
    let mut output = String::new();
    std::io::Read::read_to_string(
        &mut fixture
            .child
            .stdout
            .take()
            .context("preflight stdout missing")?,
        &mut output,
    )?;
    let report: serde_json::Value = serde_json::from_str(&output)?;
    assert_eq!(report["scope"], "native_rpc_only");
    assert_eq!(report["ok"], false);
    assert!(
        report["error"]
            .as_str()
            .is_some_and(|error| error.contains("no fallback"))
    );
    assert!(!output.contains("synthetic-secret-DO-NOT-LOG"));
    Ok(())
}
