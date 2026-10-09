#![cfg(unix)]
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct Fixture {
    _dir: tempfile::TempDir,
    home: PathBuf,
    runtime: PathBuf,
    config: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("fixture");
        let home = dir.path().join("home");
        let runtime = dir.path().join("runtime");
        std::fs::create_dir(&home).expect("home");
        std::fs::create_dir(&runtime).expect("runtime");
        std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700))
            .expect("private runtime");
        let pi = dir.path().join("synthetic-pi");
        std::fs::write(&pi, "#!/bin/sh\n: > \"$0.called\"\nif [ \"$1\" = --version ]; then echo synthetic-fixture; else exec sleep 60; fi\n").expect("synthetic Pi");
        std::fs::set_permissions(&pi, std::fs::Permissions::from_mode(0o700))
            .expect("fixture executable");
        let config = dir.path().join("config.toml");
        std::fs::write(
            &config,
            format!(
                "[local]\nsingle_user = true\nterminal_enabled = false\n[pi]\nexecutable = '{}'\n",
                pi.display()
            ),
        )
        .expect("config");
        Self {
            _dir: dir,
            home,
            runtime,
            config,
        }
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_oqto-runner"));
        command
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_RUNTIME_DIR", &self.runtime)
            .env("XDG_DATA_HOME", self.home.join("data"))
            .env("XDG_CONFIG_HOME", self.home.join("config"))
            .env("PATH", "/usr/bin:/bin")
            .args(["--no-sandbox", "--config"])
            .arg(&self.config)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        command
    }
    fn socket(&self) -> PathBuf {
        self.runtime.join("oqto-runner.sock")
    }
}
struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn wait_ready(child: &mut Owned, path: &Path) {
    for _ in 0..1500 {
        assert!(
            child.0.try_wait().expect("status").is_none(),
            "runner exited before readiness"
        );
        if let Ok(mut stream) = UnixStream::connect(path) {
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("timeout");
            stream.write_all(b"{\"type\":\"ping\"}\n").expect("ping");
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line).expect("pong");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&line).expect("json")["type"],
                "pong"
            );
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("runner not ready at {}", path.display());
}
fn terminate(child: &mut Owned) {
    assert!(
        Command::new("kill")
            .arg("-TERM")
            .arg(child.0.id().to_string())
            .status()
            .expect("signal fixture child")
            .success()
    );
    for _ in 0..100 {
        if let Some(status) = child.0.try_wait().expect("status") {
            assert!(status.success());
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("fixture child did not stop gracefully");
}

#[test]
fn default_socket_duplicate_admission_and_parent_permissions() {
    let fixture = Fixture::new();
    let path = fixture.socket();
    let mut first = Owned(fixture.command().spawn().expect("first"));
    wait_ready(&mut first, &path);
    let inode = path.metadata().expect("socket").ino();
    assert_eq!(
        fixture.runtime.metadata().expect("runtime").mode() & 0o7777,
        0o700
    );
    assert!(!fixture.runtime.join("{runtime_dir}").exists());
    let mut duplicate = Owned(fixture.command().spawn().expect("duplicate"));
    let mut refused = false;
    for _ in 0..1500 {
        if let Some(status) = duplicate.0.try_wait().expect("status") {
            assert!(!status.success());
            refused = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(refused, "duplicate daemon replaced the existing listener");
    assert_eq!(path.metadata().expect("original listener").ino(), inode);
    wait_ready(&mut first, &path);
    terminate(&mut first);
    assert!(!path.exists());
    assert!(path.with_extension("sock.lock").exists());
}

#[test]
fn shutdown_cannot_delete_a_replacement_listener() {
    let fixture = Fixture::new();
    let path = fixture.socket();
    let mut first = Owned(fixture.command().spawn().expect("first"));
    wait_ready(&mut first, &path);
    std::fs::remove_file(&path).expect("simulate legacy unlink");
    let replacement = UnixListener::bind(&path).expect("replacement listener");
    let inode = path.metadata().expect("replacement").ino();
    terminate(&mut first);
    assert_eq!(path.metadata().expect("replacement survives").ino(), inode);
    UnixStream::connect(&path).expect("replacement remains connectable");
    drop(replacement);
}

#[test]
#[ignore = "subprocess-only SDK recovery probe; exercised by the parent integration tests"]
fn sdk_recovery_probe_child() {
    let runtime = tokio::runtime::Runtime::new().expect("probe runtime");
    runtime.block_on(async {
        let failed = std::env::var("OQTO_GUARD_PROBE").expect("private probe mode") == "failure";
        let mut tasks = Vec::new();
        for _ in 0..if failed { 1 } else { 12 } {
            tasks.push(tokio::spawn(async {
                oqto_runner::client::RunnerClient::default()
                    .ensure_ready_with_recovery()
                    .await
            }));
        }
        for task in tasks {
            let result = task.await.expect("probe task");
            if failed {
                assert!(result.is_err());
            } else {
                result.expect("native recovery ready");
            }
        }
    });
}

#[cfg(target_os = "linux")]
fn sdk_probe(fixture: &Fixture, failed: bool) -> u32 {
    let bin = fixture._dir.path().join("bin");
    std::fs::create_dir(&bin).expect("bin");
    let config_dir = fixture.home.join("config/oqto");
    std::fs::create_dir_all(&config_dir).expect("private config");
    std::fs::copy(&fixture.config, config_dir.join("config.toml")).expect("config");
    let launcher = bin.join("oqto-runner");
    let action = if failed {
        "exec sleep 60".to_string()
    } else {
        format!(
            "exec '{}' --no-sandbox \"$@\"",
            env!("CARGO_BIN_EXE_oqto-runner")
        )
    };
    std::fs::write(
        &launcher,
        format!("#!/bin/sh\necho $$ >> \"$0.starts\"\n{action}\n"),
    )
    .expect("launcher");
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o700)).expect("mode");
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("HOME", &fixture.home)
        .env("XDG_RUNTIME_DIR", &fixture.runtime)
        .env("XDG_DATA_HOME", fixture.home.join("data"))
        .env("XDG_CONFIG_HOME", fixture.home.join("config"))
        .env(
            "OQTO_GUARD_PROBE",
            if failed { "failure" } else { "parallel" },
        )
        .args([
            "--exact",
            "sdk_recovery_probe_child",
            "--ignored",
            "--nocapture",
        ])
        .output()
        .expect("SDK probe");
    assert!(
        output.status.success(),
        "SDK probe failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let starts = std::fs::read_to_string(launcher.with_extension("starts")).expect("starts");
    assert_eq!(starts.lines().count(), 1, "recovery must be single-flight");
    starts.trim().parse().expect("owned fixture PID")
}

#[cfg(target_os = "linux")]
#[test]
fn parallel_sdk_recovery_starts_once_and_adopted_runner_survives_backend_exit() {
    let fixture = Fixture::new();
    let pid = sdk_probe(&fixture, false);
    let proc = PathBuf::from(format!("/proc/{pid}"));
    assert!(
        proc.exists(),
        "adopted runner must survive the SDK backend exit"
    );
    assert!(UnixStream::connect(fixture.socket()).is_ok());
    assert!(
        Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status()
            .expect("terminate owned fixture")
            .success()
    );
    for _ in 0..100 {
        if !fixture.socket().exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("adopted fixture failed graceful cleanup");
}

#[cfg(target_os = "linux")]
#[test]
fn sdk_failed_readiness_kills_and_reaps_its_live_child() {
    let fixture = Fixture::new();
    let pid = sdk_probe(&fixture, true);
    assert!(
        !Path::new(&format!("/proc/{pid}")).exists(),
        "failed recovery left its child alive or unreaped"
    );
    assert!(!fixture.socket().exists());
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires unprivileged Linux user namespaces; explicitly exercised by runtime guard gate"]
fn wrong_uid_native_launch_fails_before_state_or_workers() {
    let fixture = Fixture::new();
    let output = Command::new("unshare")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/tmp")
        .env("XDG_RUNTIME_DIR", &fixture.runtime)
        .args([
            "--user",
            "--map-root-user",
            "--",
            env!("CARGO_BIN_EXE_oqto-runner"),
            "--no-sandbox",
            "--config",
        ])
        .arg(&fixture.config)
        .output()
        .expect("namespace launch");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("refusing wrong-UID runner state access"),
        "expected real UID rejection, not harness failure: {stderr}"
    );
    assert!(!fixture.socket().exists());
    assert!(!fixture.socket().with_extension("sock.lock").exists());
    assert!(!fixture.home.join("data").exists());
    assert!(
        !fixture
            .config
            .parent()
            .expect("fixture root")
            .join("synthetic-pi.called")
            .exists(),
        "wrong-UID launch must fail before invoking Pi during config resolution"
    );
}
