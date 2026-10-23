//! Disposable process proof: hot-enable on the same PID, no real keys/provider/Pi.
use anyhow::{Result, ensure};
use clap::Parser;
use oqto_runner::{
    pair_cli::{Access, Files, PairOptions, canonical_roots, grants},
    pair_control::{self, ListenerPlan, Request},
    personal_cli::{GrantSelection, Sessions},
};
use std::{
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

#[derive(Parser)]
struct Options {
    #[command(flatten)]
    options: PairOptions,
}
fn parse(args: &[&str]) -> Result<PairOptions> {
    Ok(Options::try_parse_from(std::iter::once("pair").chain(args.iter().copied()))?.options)
}
#[test]
fn headless_requires_complete_explicit_confirmed_options() -> Result<()> {
    let options = parse(&[])?;
    ensure!(options.mode(true).map_err(anyhow::Error::msg)?);
    ensure!(options.mode(false).is_err());
    let options = parse(&[
        "--workspace",
        "/tmp",
        "--access",
        "full",
        "--network",
        "tailscale",
        "--non-interactive",
    ])?;
    ensure!(options.mode(true).is_err());
    let options = parse(&[
        "--workspace",
        "/tmp",
        "--access",
        "full",
        "--network",
        "tailscale",
        "--yes",
    ])?;
    ensure!(!options.mode(false).map_err(anyhow::Error::msg)?);
    let options = parse(&["--interactive"])?;
    ensure!(options.mode(false).is_err());
    ensure!(parse(&["--interactive", "--non-interactive"]).is_err());
    ensure!(parse(&["--port", "0"]).is_err());
    Ok(())
}
#[test]
fn flags_plan_is_explicit_and_roots_are_canonical() -> Result<()> {
    let home = tempfile::tempdir()?;
    std::fs::create_dir(home.path().join("work"))?;
    symlink(home.path().join("work"), home.path().join("alias"))?;
    let roots = canonical_roots(
        &[PathBuf::from("~/work"), home.path().join("alias")],
        home.path(),
    )?;
    ensure!(roots.len() == 1);
    let mut options = parse(&["--access", "full"])?;
    let full = grants(&options, &roots);
    ensure!(
        matches!(full.sessions, Sessions::Control)
            && full.full_user_terminal
            && full.write_roots == roots
    );
    options.access = Some(Access::ReadOnly);
    let read = grants(&options, &roots);
    ensure!(
        matches!(read.sessions, Sessions::Observe)
            && !read.full_user_terminal
            && read.read_roots == roots
    );
    options.files = Some(Files::None);
    options.sessions = Some(Sessions::None);
    options.terminal = Some(false);
    let none = grants(&options, &roots);
    ensure!(none.read_roots.is_empty() && none.write_roots.is_empty() && !none.full_user_terminal);
    ensure!(canonical_roots(&[], home.path()).is_err());
    ensure!(canonical_roots(&[home.path().join("missing")], home.path()).is_err());
    Ok(())
}
#[test]
fn mac_managed_profile_paths_fit_portable_unix_socket_limit() -> Result<()> {
    let home = Path::new("/Users/tommyfalkowski/.local/share/oqto/mac-runner/home");
    let root = pair_control::root(home);
    for name in ["control.sock", "native.sock", "host/owner.sock"] {
        pair_control::validate_socket_path(&root.join(name))?;
    }
    ensure!(
        pair_control::validate_socket_path(&PathBuf::from("/".to_owned() + &"x".repeat(104)))
            .is_err()
    );
    Ok(())
}
#[test]
fn state_symlinks_and_lock_contention_fail_without_mutating_targets() -> Result<()> {
    // flock is tied to the open description: another parallel test's fork can
    // briefly inherit it before CLOEXEC. Assert release in an isolated process,
    // not against an unrelated fork/exec window in the test harness.
    const CHILD: &str = "OQTO_PAIR_LOCK_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "state_symlinks_and_lock_contention_fail_without_mutating_targets",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .status()?;
        ensure!(status.success(), "isolated pairing lock test failed");
        return Ok(());
    }
    let home = tempfile::tempdir()?;
    let canonical_home = home.path().canonicalize()?;
    let target = canonical_home.join("target");
    std::fs::create_dir(&target)?;
    let alias = canonical_home.join("alias");
    symlink(&target, &alias)?;
    ensure!(pair_control::private_dir(&alias.join("new")).is_err());
    ensure!(!target.join("new").exists());
    let root = canonical_home.join("private");
    pair_control::private_dir(&root)?;
    let lock = pair_control::lock(&root.join("lock"))?;
    ensure!(pair_control::lock(&root.join("lock")).is_err());
    drop(lock);
    ensure!(pair_control::lock(&root.join("lock")).is_ok());
    symlink(root.join("foreign"), root.join("bad-lock"))?;
    ensure!(pair_control::lock(&root.join("bad-lock")).is_err());
    ensure!(!root.join("foreign").exists());
    Ok(())
}
struct Host {
    child: Child,
    home: PathBuf,
    socket: PathBuf,
    config: PathBuf,
}
impl Host {
    fn start(home: &Path) -> Result<Self> {
        let socket = home.join("runner.sock");
        let config = home.join("config.toml");
        if !config.exists() {
            std::fs::write(
                &config,
                "[local]\nsingle_user=true\nterminal_enabled=true\n[local.linux_users]\nenabled=false\n[pi]\nexecutable='pi-must-not-be-launched'\n",
            )?;
        }
        let child = Command::new(env!("CARGO_BIN_EXE_oqto-runner"))
            .args(["--no-sandbox", "--config"])
            .arg(&config)
            .arg("--socket")
            .arg(&socket)
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_RUNTIME_DIR", home.join("runtime"))
            .env("TMPDIR", home)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(home.join("fixture-stderr.log"))?)
            .spawn()?;
        Ok(Self {
            child,
            home: home.to_owned(),
            socket,
            config,
        })
    }
    async fn ready(&mut self) -> Result<()> {
        for _ in 0..100 {
            ensure!(
                self.child.try_wait()?.is_none(),
                "fixture Runner exited: {}",
                std::fs::read_to_string(self.home.join("fixture-stderr.log")).unwrap_or_default()
            );
            if pair_control::request(&pair_control::root(&self.home), &Request::Status)
                .await
                .is_ok()
            {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        anyhow::bail!("fixture controller readiness timeout")
    }
    fn stop(&mut self) -> Result<()> {
        let status = Command::new("kill")
            .args(["-INT", &self.child.id().to_string()])
            .status()?;
        ensure!(status.success(), "could not signal owned fixture");
        for _ in 0..100 {
            if self.child.try_wait()?.is_some() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        anyhow::bail!("owned fixture shutdown timeout")
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn plan(home: &Path) -> Result<ListenerPlan> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    Ok(ListenerPlan {
        address,
        origin: format!("https://{address}/"),
        workspaces: vec![home.join("work").canonicalize()?],
        native_home: home.to_owned(),
    })
}
#[tokio::test]
async fn hot_enable_same_pid_pair_and_restart_preserve_identity() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let home = temp.path().canonicalize()?;
    std::fs::create_dir(home.join("work"))?;
    std::fs::create_dir(home.join("runtime"))?;
    std::fs::set_permissions(home.join("runtime"), std::fs::Permissions::from_mode(0o700))?;
    let mut host = Host::start(&home)?;
    host.ready().await?;
    let pid = host.child.id();
    let control = pair_control::root(&home);
    let selected = plan(&home)?;
    let activated = pair_control::request(
        &control,
        &Request::Enable {
            plan: selected.clone(),
        },
    )
    .await?;
    ensure!(activated.pid == pid);
    let repeated = pair_control::request(
        &control,
        &Request::Enable {
            plan: selected.clone(),
        },
    )
    .await?;
    ensure!(repeated.pid == pid);
    let mut widened = selected.clone();
    std::fs::create_dir(home.join("other"))?;
    widened.workspaces.push(home.join("other"));
    ensure!(
        pair_control::request(&control, &Request::Enable { plan: widened })
            .await
            .is_err()
    );
    let grant = GrantSelection {
        sessions: Sessions::Control,
        write_roots: selected.workspaces.clone(),
        full_user_terminal: true,
        ..Default::default()
    };
    let first = oqto_personal::owner::request_owner(&activated.state, &grant.command()).await?;
    ensure!(first["purpose"] == "oqto.personal-host.v1" && first["url"] == selected.origin);
    ensure!(host.socket.exists() && host.config.exists());
    host.stop()?;
    let mut restarted = Host::start(&home)?;
    restarted.ready().await?;
    let restored = pair_control::request(&control, &Request::Status).await?;
    ensure!(restored.plan == Some(selected));
    let second = oqto_personal::owner::request_owner(&restored.state, &grant.command()).await?;
    ensure!(
        first["host_id"] == second["host_id"]
            && first["pin"] == second["pin"]
            && first["token"] != second["token"]
    );
    restarted.stop()?;
    Ok(())
}
#[tokio::test]
async fn prebound_split_source_hot_enables_without_changing_service_home() -> Result<()> {
    // Nested service HOME must fit the real portable Unix-socket bound even
    // when the qualification harness itself has a long private TMPDIR.
    let temp = tempfile::Builder::new()
        .prefix("oqpair-bind-")
        .tempdir_in("/tmp")?;
    let root = temp.path().canonicalize()?;
    let home = root.join("service");
    let native = root.join("native-user");
    let sessions = native.join(".pi/agent/sessions");
    let sockets = root.join("native-tmp");
    let work = native.join("work");
    for directory in [&home, &sessions, &sockets, &work, &home.join("runtime")] {
        std::fs::create_dir_all(directory)?;
    }
    std::fs::set_permissions(home.join("runtime"), std::fs::Permissions::from_mode(0o700))?;
    let binding = oqto_runner::personal_source::binding::Binding {
        schema: 1,
        session_root: sessions.clone(),
        socket_root: sockets,
        workspaces: vec![work.clone()],
    };
    let configured = Command::new(env!("CARGO_BIN_EXE_oqto-runner"))
        .arg("configure-native-source")
        .arg("--runner-home")
        .arg(&home)
        .arg("--session-root")
        .arg(&binding.session_root)
        .arg("--socket-root")
        .arg(&binding.socket_root)
        .arg("--workspace-root")
        .arg(&work)
        .env("HOME", &home)
        .stdin(Stdio::null())
        .output()?;
    ensure!(
        configured.status.success(),
        "source CLI failed: {}",
        String::from_utf8_lossy(&configured.stderr)
    );
    let configured: serde_json::Value = serde_json::from_slice(&configured.stdout)?;
    ensure!(configured["configured"] == true && configured["activation"] == "on_next_start");
    let bucket = sessions.join(oqto_pi::session_files::cwd_to_safe_dirname(&work));
    std::fs::create_dir(&bucket)?;
    let file = bucket.join("2026-10-20_binding-native.jsonl");
    let bytes = format!(
        "{}\n",
        serde_json::json!({"type":"session","version":3,"id":"binding-native","cwd":work})
    )
    .into_bytes();
    std::fs::write(&file, &bytes)?;
    let mut host = Host::start(&home)?;
    host.ready().await?;
    let pid = host.child.id();
    let selected = plan(&native)?;
    let activated = pair_control::request(
        &pair_control::root(&home),
        &Request::Enable { plan: selected },
    )
    .await?;
    ensure!(activated.active && activated.pid == pid && activated.state.starts_with(&home));
    let grant = GrantSelection {
        sessions: Sessions::Control,
        write_roots: vec![work],
        full_user_terminal: true,
        ..Default::default()
    };
    let ticket = oqto_personal::owner::request_owner(&activated.state, &grant.command()).await?;
    ensure!(ticket["purpose"] == "oqto.personal-host.v1");
    ensure!(std::fs::read(&file)? == bytes);
    ensure!(host.socket.starts_with(&home) && !native.join(".local").exists());
    host.stop()?;
    Ok(())
}

#[tokio::test]
async fn occupied_port_and_native_home_mismatch_leave_primary_runner_alive() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let home = temp.path().canonicalize()?;
    std::fs::create_dir(home.join("work"))?;
    std::fs::create_dir(home.join("runtime"))?;
    std::fs::set_permissions(home.join("runtime"), std::fs::Permissions::from_mode(0o700))?;
    let mut host = Host::start(&home)?;
    host.ready().await?;
    let control = pair_control::root(&home);
    let occupied = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = occupied.local_addr()?;
    let bad = ListenerPlan {
        address,
        origin: format!("https://{address}/"),
        ..plan(&home)?
    };
    ensure!(
        pair_control::request(&control, &Request::Enable { plan: bad })
            .await
            .is_err()
    );
    ensure!(!control.join("host").exists() && !control.join("listener.json").exists());
    let mut mismatch = plan(&home)?;
    mismatch.native_home = home.join("work");
    ensure!(
        pair_control::request(&control, &Request::Enable { plan: mismatch })
            .await
            .is_err()
    );
    let status = pair_control::request(&control, &Request::Status).await?;
    ensure!(status.pid == host.child.id() && status.plan.is_none());
    host.stop()?;
    Ok(())
}
#[test]
fn real_headless_cli_errors_are_usage_errors_and_do_not_create_state() -> Result<()> {
    let home = tempfile::tempdir()?;
    for args in [
        vec!["pair"],
        vec!["pair", "--interactive"],
        vec!["pair", "--non-interactive"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_oqto-runner"))
            .args(args)
            .env("HOME", home.path())
            .stdin(Stdio::null())
            .output()?;
        ensure!(
            output.status.code() == Some(2) && output.stdout.is_empty(),
            "expected usage error before mutation"
        );
    }
    let json = Command::new(env!("CARGO_BIN_EXE_oqto-runner"))
        .args(["pair", "--output", "json"])
        .env("HOME", home.path())
        .stdin(Stdio::null())
        .output()?;
    ensure!(json.status.code() == Some(2));
    let failure: serde_json::Value = serde_json::from_slice(&json.stdout)?;
    ensure!(failure["ok"] == false && failure["error"]["code"] == "usage");
    ensure!(!home.path().join(".local").exists());
    Ok(())
}

#[path = "pair_wizard/output.rs"]
mod output;
#[path = "pair_wizard/recovery.rs"]
mod recovery;
#[path = "pair_wizard/transient.rs"]
mod transient;
