//! Full process proof: service HOME/TMPDIR differ from terminal source roots.
//! No identity row is preseeded. All peers, profiles and grants are disposable.
use super::*;
use oqto_personal_wire::actions::SessionInput;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::net::UnixListener;

const NATIVE: &str = "split-native";
const FRESH: &str = "fresh-header-only";
fn source_file(root: &Path, work: &Path, id: &str, body: &str) -> Result<PathBuf> {
    let dir = root.join(oqto_pi::session_files::cwd_to_safe_dirname(work));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("2026-10-19_{id}.jsonl"));
    std::fs::write(
        &path,
        format!(
            "{}\n{body}",
            serde_json::json!({"type":"session","version":3,"id":id,"cwd":work})
        ),
    )?;
    Ok(path)
}
fn free_address() -> Result<String> {
    let reserved = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(reserved.local_addr()?.to_string())
}
struct Fixture {
    host: Host,
    args: Vec<String>,
    root: PathBuf,
    source: PathBuf,
    user: PathBuf,
    sockets: PathBuf,
    mtls: String,
    config_bytes: Vec<u8>,
    transport_bytes: [Vec<u8>; 2],
    native_bytes: Vec<u8>,
    retained_shards: Vec<(PathBuf, Vec<u8>)>,
    broken_shard: Option<PathBuf>,
    inventory_fixture: bool,
}
impl Fixture {
    async fn start() -> Result<Self> {
        Self::start_with_inventory(0, false).await
    }
    async fn start_with_inventory(shards: usize, broken: bool) -> Result<Self> {
        Self::start_with_options(shards, broken, false).await
    }
    async fn start_with_options(shards: usize, broken: bool, terminal: bool) -> Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("oqsplit-")
            .tempdir_in("/tmp")?;
        let root = dir.path().canonicalize()?;
        let (home, work, state, user, sockets) = (
            root.join("service"),
            root.join("byteowlz"),
            root.join("state"),
            root.join("native-user"),
            root.join("terminal-tmp"),
        );
        let native_root = user.join(".pi/agent/sessions");
        for path in [&home.join("tmp"), &work, &native_root, &sockets] {
            std::fs::create_dir_all(path)?;
        }
        let message = serde_json::json!({"role":"user","content":[{"type":"text","text":"same text, distinct entries"}],"timestamp":1720000000000_i64});
        let body = format!(
            "{}\n{}\n{}\n",
            serde_json::json!({"type":"custom","id":"structural","parentId":null,"customType":"fixture","data":{}}),
            serde_json::json!({"type":"message","id":"a","parentId":"structural","message":message}),
            serde_json::json!({"type":"message","id":"b","parentId":"structural","message":message})
        );
        let source = source_file(&native_root, &work, NATIVE, &body)?;
        source_file(&native_root, &work, FRESH, "")?;
        let outside = root.join("outside");
        std::fs::create_dir_all(&outside)?;
        source_file(
            &native_root,
            &outside,
            "outside-native",
            "NOT JSON: must not parse an outside body\n",
        )?;
        std::fs::write(
            user.join(".pi/agent/profile-sentinel"),
            b"private synthetic profile: unchanged",
        )?;
        // Retain the original default source and its sixteen-session catalog.
        for n in 0..16 {
            let path = source_file(
                &home.join(".pi/agent/sessions"),
                &work,
                &format!("default-{n}"),
                &format!(
                    "{}\n",
                    serde_json::json!({"type":"message","id":"legacy","message":message})
                ),
            )?;
            std::fs::File::open(path)?
                .set_modified(SystemTime::now() - Duration::from_secs(120))?;
        }
        // Actual retained migration prefixes, with NO source JSONL to upgrade
        // them via the default writer. Fresh sources live in another store.
        let mut retained_shards = Vec::new();
        for (name, prefix) in [("april", 1), ("july", 3)] {
            let workspace = work.join(name);
            std::fs::create_dir(&workspace)?;
            let db = oqto_history::oqto_log::paths::resolve_user_home_workspace_db_path(
                &home,
                workspace.to_str().context("workspace")?,
            )?;
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(
                    sqlx::sqlite::SqliteConnectOptions::new()
                        .filename(&db)
                        .create_if_missing(true),
                )
                .await?;
            let mut migrator = sqlx::migrate!("../oqto-history/migrations_oqto_log");
            migrator.migrations =
                std::borrow::Cow::Owned(migrator.iter().take(prefix).cloned().collect());
            migrator.run(&pool).await?;
            for kind in ["parent", "child"] {
                let session = format!("retained-{name}-{kind}");
                sqlx::query("INSERT INTO oqto_log_sessions(session_id,platform_id,external_id,user_id,workspace_id) VALUES(?,?,?,?,?)")
                    .bind(&session).bind(format!("oqto-{session}"))
                    .bind(format!("absent-native-{name}-{kind}"))
                    .bind("retained-owner").bind(workspace.to_str()).execute(&pool).await?;
                sqlx::query("INSERT INTO oqto_log_branches(branch_id,session_id) VALUES(?,?)")
                    .bind(format!("branch-{session}"))
                    .bind(&session)
                    .execute(&pool)
                    .await?;
                sqlx::query("INSERT INTO oqto_log_turns(turn_id,session_id,branch_id,turn_version,role,status,source_kind,source_session_id,source_entry_id,source_hash) VALUES(?,?,?,1,'user','committed','pi_jsonl',?,'retained-entry','retained-hash')")
                    .bind(format!("turn-{session}")).bind(&session).bind(format!("branch-{session}"))
                    .bind(format!("absent-native-{name}-{kind}")).execute(&pool).await?;
                sqlx::query("INSERT INTO oqto_log_messages(message_id,turn_id,seq,kind,role,content) VALUES(?,?,0,'text','user','retained synthetic history')")
                    .bind(format!("message-{session}")).bind(format!("turn-{session}")).execute(&pool).await?;
            }
            if name == "july" {
                sqlx::query("UPDATE oqto_log_sessions SET parent_session_id='retained-july-parent',forked_from_entry_id='retained-entry',forked_at='2026-07-19',fork_operation_id='retained-fork',extensions_json='{\"provenance\":\"retained\"}' WHERE session_id='retained-july-child'")
                    .execute(&pool).await?;
            }
            pool.close().await;
            retained_shards.push((db.clone(), std::fs::read(db)?));
        }
        // Cold reader must list both prefixes without upgrading either store.
        let cold = oqto_history::oqto_log::ops::list_sessions(&home, None).await?;
        assert_eq!(cold.len(), 4);
        let child = cold
            .iter()
            .find(|row| row.platform_id == "oqto-retained-july-child")
            .context("retained Fork")?;
        assert_eq!(
            child.parent_session_id.as_deref(),
            Some("oqto-retained-july-parent")
        );
        assert_eq!(
            child.forked_from_entry_id.as_deref(),
            Some("retained-entry")
        );
        assert!(cold.iter().all(|row| row.user_id == "retained-owner"));
        for (path, bytes) in &retained_shards {
            assert_eq!(&std::fs::read(path)?, bytes);
        }
        // Many canonical shards outside the approved source roots still belong
        // to GLOBAL identity authority. No source/profile is read to build them.
        let mut broken_shard = None;
        for n in 0..shards {
            let workspace = root.join("outside-inventory").join(format!("shard-{n}"));
            let db = oqto_history::oqto_log::paths::resolve_user_home_workspace_db_path(
                &home,
                workspace.to_str().context("inventory Workspace")?,
            )?;
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(
                    sqlx::sqlite::SqliteConnectOptions::new()
                        .filename(&db)
                        .create_if_missing(true),
                )
                .await?;
            let mut migrator = sqlx::migrate!("../oqto-history/migrations_oqto_log");
            migrator.migrations =
                std::borrow::Cow::Owned(migrator.iter().take(1).cloned().collect());
            migrator.run(&pool).await?;
            sqlx::query("INSERT INTO oqto_log_sessions(session_id,platform_id,external_id,user_id,workspace_id) VALUES(?,?,?,'inventory-owner',?)")
                .bind(format!("retained-inventory-{n}")).bind(format!("oqto-retained-inventory-{n}"))
                .bind(format!("outside-native-{n}")).bind(workspace.to_str()).execute(&pool).await?;
            pool.close().await;
            retained_shards.push((db.clone(), std::fs::read(&db)?));
            if broken && n == shards / 2 {
                std::fs::write(&db, b"injected unreadable SQLite authority")?;
                broken_shard = Some(db);
            }
        }
        let sentinel = home.join("pi-sentinel");
        std::fs::write(
            &sentinel,
            format!(
                "#!/bin/sh\nprintf invoked >> '{}'\nexit 97\n",
                home.join("pi-invocations").display()
            ),
        )?;
        std::fs::set_permissions(&sentinel, std::fs::Permissions::from_mode(0o700))?;
        let config = root.join("config.toml");
        let config_bytes = format!("[local]\nsingle_user=true\nterminal_enabled={terminal}\nworkspace_dir={}\n[runner]\nrunner_id=\"split-fixture\"\n[pi]\nexecutable={}\n[unknown_future]\npreserve=\"opaque synthetic value\"\n", serde_json::to_string(&work)?, serde_json::to_string(&sentinel)?).into_bytes();
        std::fs::write(&config, &config_bytes)?;
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600))?;
        let identity = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
        std::fs::write(root.join("mtls.pem"), identity.cert.pem())?;
        std::fs::write(root.join("mtls.key"), identity.key_pair.serialize_pem())?;
        std::fs::set_permissions(
            root.join("mtls.key"),
            std::fs::Permissions::from_mode(0o600),
        )?;
        let (personal, mtls) = (free_address()?, free_address()?);
        let args = vec![
            "--config".into(),
            config.to_string_lossy().into(),
            "--no-sandbox".into(),
            "--listen-tls".into(),
            mtls.clone(),
            "--tls-cert".into(),
            root.join("mtls.pem").to_string_lossy().into(),
            "--tls-key".into(),
            root.join("mtls.key").to_string_lossy().into(),
            "--tls-client-ca".into(),
            root.join("mtls.pem").to_string_lossy().into(),
            "--personal-listen".into(),
            personal.clone(),
            "--personal-origin".into(),
            format!("https://{personal}"),
            "--personal-state".into(),
            state.to_string_lossy().into(),
            "--personal-workspace-root".into(),
            work.to_string_lossy().into(),
            "--personal-native-session-root".into(),
            native_root.to_string_lossy().into(),
            "--personal-native-socket-root".into(),
            sockets.to_string_lossy().into(),
        ];
        let mut process = command(&home);
        if shards > 0 {
            process.env("TOKIO_WORKER_THREADS", "2");
        }
        let mut child = process
            .args(&args)
            .args(["--sessions", "control", "--pair-output", "json"])
            .stdout(Stdio::piped())
            .stderr(std::fs::File::create(root.join("startup.log"))?)
            .spawn()?;
        let mut lines = BufReader::new(child.stdout.take().context("stdout")?).lines();
        let qr = tokio::time::timeout(Duration::from_secs(20), lines.next_line())
            .await??
            .context("QR")?;
        let qr = PairingQr::parse(qr.as_bytes(), now()?)?;
        let native_bytes = std::fs::read(&source)?;
        let transport_bytes = [
            std::fs::read(root.join("mtls.pem"))?,
            std::fs::read(root.join("mtls.key"))?,
        ];
        Ok(Self {
            host: Host {
                _dir: dir,
                home,
                work,
                state,
                child,
                qr,
            },
            args,
            root,
            source,
            user,
            sockets,
            mtls,
            config_bytes,
            transport_bytes,
            native_bytes,
            retained_shards,
            broken_shard,
            inventory_fixture: shards > 0,
        })
    }
    async fn primary(&self) -> Result<()> {
        use oqto_runner::transport::RunnerConnector;
        let connector = oqto_runner::tls::TcpTlsRunnerConnector::new(
            self.mtls.parse()?,
            "localhost",
            oqto_runner::tls::client_config(
                &self.root.join("mtls.pem"),
                &self.root.join("mtls.pem"),
                &self.root.join("mtls.key"),
            )?,
        );
        let mut stream = BufReader::new(connector.connect().await?);
        oqto_runner::wire::write_json_frame(
            stream.get_mut(),
            &oqto_runner::protocol::RunnerRequest::Ping,
        )
        .await?;
        let pong: Option<oqto_runner::protocol::RunnerResponse> =
            oqto_runner::wire::read_json_frame(&mut stream).await?;
        assert!(matches!(
            pong,
            Some(oqto_runner::protocol::RunnerResponse::Pong)
        ));
        oqto_runner::wire::write_json_frame(
            stream.get_mut(),
            &oqto_runner::protocol::RunnerRequest::PiCreateSession(
                oqto_runner::protocol::PiCreateSessionRequest {
                    session_id: "denied-managed".into(),
                    config: oqto_runner::protocol::PiSessionConfig {
                        cwd: self.host.work.clone(),
                        ..Default::default()
                    },
                },
            ),
        )
        .await?;
        let denied: Option<oqto_runner::protocol::RunnerResponse> =
            oqto_runner::wire::read_json_frame(&mut stream).await?;
        assert!(matches!(
            denied,
            Some(oqto_runner::protocol::RunnerResponse::Error { .. })
        ));
        Ok(())
    }
    async fn restart(&mut self) -> Result<()> {
        let pid = rustix::process::Pid::from_raw(self.host.child.id().context("child id")? as i32)
            .context("pid")?;
        rustix::process::kill_process(pid, rustix::process::Signal::TERM)?;
        assert!(
            tokio::time::timeout(Duration::from_secs(10), self.host.child.wait())
                .await??
                .success()
        );
        let mut process = command(&self.host.home);
        if self.inventory_fixture {
            process.env("TOKIO_WORKER_THREADS", "2");
        }
        self.host.child = process
            .args(&self.args)
            .arg("--personal-no-pair")
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(self.root.join("restart.log"))?)
            .spawn()?;
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if self.host.owner(&["devices"]).await.is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await?;
        Ok(())
    }
}
#[tokio::test]
async fn invalid_explicit_native_roots_abort_full_startup_before_workers_or_pi() -> Result<()> {
    let dir = tempfile::Builder::new()
        .prefix("oqdenied-")
        .tempdir_in("/tmp")?;
    let root = dir.path().canonicalize()?;
    let (home, sessions, work, sockets) = (
        root.join("home"),
        root.join("sessions"),
        root.join("work"),
        root.join("sockets"),
    );
    for path in [&home, &sessions, &work, &sockets] {
        std::fs::create_dir(path)?;
    }
    let executable = home.join("pi");
    std::fs::write(
        &executable,
        format!(
            "#!/bin/sh\nprintf called >> '{}'\nexit 1\n",
            home.join("invoked").display()
        ),
    )?;
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))?;
    let config = home.join("config.toml");
    std::fs::write(
        &config,
        format!(
            "[local]\nsingle_user=true\n[pi]\nexecutable={}\n",
            serde_json::to_string(&executable)?
        ),
    )?;
    symlink(&sessions, root.join("source-alias"))?;
    symlink(&sockets, root.join("socket-alias"))?;
    let mut denied = vec![
        (PathBuf::from("relative"), sockets.clone()),
        (root.join("source-alias"), sockets.clone()),
        (sessions.clone(), root.join("socket-alias")),
    ];
    // On the qualified macOS user lane, '/' supplies a real foreign-owned
    // directory without privilege changes or reading any private source.
    if rustix::process::geteuid().as_raw() != 0 {
        denied.push((PathBuf::from("/"), sockets.clone()));
    }
    for (session_root, socket_root) in denied {
        let output = command(&home)
            .arg("--config")
            .arg(&config)
            .args([
                "--no-sandbox",
                "--personal-listen",
                "127.0.0.1:0",
                "--personal-origin",
                "https://localhost:39444",
                "--personal-no-pair",
                "--personal-workspace-root",
            ])
            .arg(&work)
            .arg("--personal-native-session-root")
            .arg(session_root)
            .arg("--personal-native-socket-root")
            .arg(socket_root)
            .output()
            .await?;
        assert!(!output.status.success());
        assert!(!home.join("invoked").exists());
        assert!(!home.join(".local").exists());
        assert!(!home.join(".pi").exists());
    }
    Ok(())
}

struct Peer {
    head: Arc<AtomicBool>,
    uncertain: Arc<AtomicBool>,
    writes: Arc<AtomicUsize>,
    leases: Arc<AtomicUsize>,
    active_leases: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Peer {
    fn start(fixture: &Fixture) -> Result<Self> {
        let path = fixture
            .sockets
            .join(format!("pi-tui-rpc-{}.sock", std::process::id()));
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        let (head, uncertain, writes, leases, active_leases) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
        );
        let state = (
            head.clone(),
            uncertain.clone(),
            writes.clone(),
            leases.clone(),
            active_leases.clone(),
        );
        // Pi retains its logical directory spelling while the owner configures
        // the canonical native root. The entire signed HTTPS history/control
        // process proof must work through that runtime-only alias.
        let native_root = fixture.user.join(".pi/agent/sessions");
        let alias = fixture.root.join("runtime-sessions");
        std::os::unix::fs::symlink(&native_root, &alias)?;
        let source = alias.join(fixture.source.strip_prefix(&native_root)?);
        let cwd = fixture.host.work.clone();
        let task = tokio::spawn(async move {
            let mut children = tokio::task::JoinSet::new();
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let ((head, uncertain, writes, leases, active), source, cwd) =
                    (state.clone(), source.clone(), cwd.clone());
                children.spawn(async move {
                    let mut stream = BufReader::new(stream);
                    let hello = serde_json::json!({"type":"hello","v":1,"pid":std::process::id(),"commands":["get_state","lease","prompt","steer","abort"],"capabilities":["connection_leases_v1"]});
                    stream.get_mut().write_all(format!("{hello}\n").as_bytes()).await?;
                    struct Lease(Arc<AtomicUsize>);
                    impl Drop for Lease { fn drop(&mut self) { self.0.fetch_sub(1, Ordering::SeqCst); } }
                    let mut held = None;
                    let mut line = String::new();
                    while stream.read_line(&mut line).await? != 0 {
                        let frame: Value = serde_json::from_str(&line)?;
                        line.clear();
                        let mut response = serde_json::json!({"type":"response","id":frame["id"],"command":frame["type"],"success":true});
                        match frame["type"].as_str() {
                            Some("get_state") => response["data"] = serde_json::json!({"sessionId":NATIVE,"sessionFile":source,"cwd":cwd,"leafId":if head.load(Ordering::SeqCst) { "b" } else { "a" }}),
                            Some("lease") => { leases.fetch_add(1, Ordering::SeqCst); active.fetch_add(1, Ordering::SeqCst); held=Some(Lease(active.clone())); response["data"] = serde_json::json!({"owner":"remote","reason":"tui_confirmed"}); }
                            Some("prompt" | "steer" | "abort") if held.is_some() => { writes.fetch_add(1, Ordering::SeqCst); if uncertain.load(Ordering::SeqCst) { break; } }
                            _ => response["success"] = false.into(),
                        }
                        stream.get_mut().write_all(format!("{response}\n").as_bytes()).await?;
                    }
                    Ok::<(), anyhow::Error>(())
                });
            }
        });
        Ok(Self {
            head,
            uncertain,
            writes,
            leases,
            active_leases,
            task,
        })
    }
}
async fn attach(host: &Host, device: &str, seed: u8, public: &str) -> Result<String> {
    let (status, result) = host
        .action(
            device,
            seed,
            HostAction::SessionAttach {
                session_id: public.into(),
            },
        )
        .await?;
    assert_eq!(status, 200, "{result}");
    Ok(result["attachment_id"]
        .as_str()
        .context("attachment")?
        .into())
}
async fn page(host: &Host, device: &str, public: &str, head: &str) -> Result<Value> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (status, page) = host
                .action(
                    device,
                    101,
                    HostAction::SessionHistory {
                        session_id: public.into(),
                        before: None,
                    },
                )
                .await?;
            if status == 200 && page["native_graph"]["native_leaf_id"] == head {
                break Ok::<_, anyhow::Error>(page);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?
}
// Inspect only the exact disposable child created by this fixture, never the
// deployed Runner or another process. No OS limits are raised.
fn synthetic_child_resources(pid: u32) -> Result<(usize, usize)> {
    #[cfg(target_os = "macos")]
    {
        let threads = std::process::Command::new("ps")
            .args(["-M", "-p", &pid.to_string(), "-o", "pid="])
            .output()?;
        ensure!(
            threads.status.success(),
            "synthetic child thread inspection failed"
        );
        let thread_sample = String::from_utf8(threads.stdout)?;
        let threads = thread_sample
            .lines()
            .filter(|line| {
                line.split_whitespace()
                    .last()
                    .is_some_and(|field| field.parse::<u32>() == Ok(pid))
            })
            .count();
        let fds = std::process::Command::new("/usr/sbin/lsof")
            .args(["-n", "-P", "-p", &pid.to_string(), "-Ff"])
            .output()?;
        ensure!(fds.status.success(), "synthetic child fd inspection failed");
        let fds = String::from_utf8(fds.stdout)?
            .lines()
            .filter(|line| {
                line.strip_prefix('f')
                    .is_some_and(|fd| !fd.is_empty() && fd.bytes().all(|b| b.is_ascii_digit()))
            })
            .count();
        ensure!(
            threads >= 3 && fds >= 3,
            "resource sample must include real threads/descriptors: threads={threads}, fds={fds}, ps={thread_sample:?}"
        );
        Ok((threads, fds))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let base = PathBuf::from(format!("/proc/{pid}"));
        Ok((
            std::fs::read_dir(base.join("task"))?.count(),
            std::fs::read_dir(base.join("fd"))?.count(),
        ))
    }
}

#[tokio::test]
async fn many_shard_failure_refuses_sessions_recovers_and_bounds_process_resources() -> Result<()> {
    const SHARDS: usize = 192;
    let fixture = Fixture::start_with_inventory(SHARDS, true).await?;
    let peer = Peer::start(&fixture)?;
    let device = fixture.host.enroll(&fixture.host.qr, 101).await?;
    let sampling = Arc::new(AtomicBool::new(true));
    let active = sampling.clone();
    let pid = fixture.host.child.id().context("synthetic child pid")?;
    let sampler = tokio::spawn(async move {
        let mut peak = (0, 0);
        while active.load(Ordering::SeqCst) {
            let (threads, fds) =
                tokio::task::spawn_blocking(move || synthetic_child_resources(pid)).await??;
            peak.0 = peak.0.max(threads);
            peak.1 = peak.1.max(fds);
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        Ok::<_, anyhow::Error>(peak)
    });
    let (_, caps) = fixture
        .host
        .action(&device, 101, HostAction::Capabilities)
        .await?;
    // The installed strict shared decoder accepts this exact existing shape in
    // both failed and recovered states; readiness adds NO capability fields.
    serde_json::from_value::<oqto_personal_wire::actions::HostCapabilities>(caps.clone())?;
    let (status, failure) = fixture
        .host
        .action(&device, 101, HostAction::SessionsList)
        .await?;
    assert_eq!(
        status, 503,
        "an incomplete authority must not serve an old/partial successful catalog: {failure}"
    );
    let public = oqto_history::oqto_log::store::platform_id_for_external_id(NATIVE);
    assert_eq!(
        fixture
            .host
            .action(
                &device,
                101,
                HostAction::SessionAttach {
                    session_id: public.clone()
                }
            )
            .await?
            .0,
        503
    );
    let destination = oqto_history::oqto_log::paths::existing_user_home_workspace_db_path(
        &fixture.host.home,
        fixture.host.work.to_str().context("Workspace")?,
    );
    assert!(
        !destination.exists(),
        "initial inventory failure must precede any fresh adoption"
    );
    let bad = fixture.broken_shard.as_ref().context("injected shard")?;
    let bytes = &fixture
        .retained_shards
        .iter()
        .find(|(path, _)| path == bad)
        .context("retained bytes")?
        .1;
    std::fs::write(bad, bytes)?;
    tokio::time::timeout(Duration::from_secs(25), async {
        loop {
            let (status, list) = fixture
                .host
                .action(&device, 101, HostAction::SessionsList)
                .await?;
            if status == 200
                && list["sessions"]
                    .as_array()
                    .is_some_and(|rows| rows.len() == 22)
            {
                break Ok::<_, anyhow::Error>(());
            }
            if status != 200 {
                assert_eq!(status, 503);
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    })
    .await??;
    let (_, recovered_caps) = fixture
        .host
        .action(&device, 101, HostAction::Capabilities)
        .await?;
    serde_json::from_value::<oqto_personal_wire::actions::HostCapabilities>(
        recovered_caps.clone(),
    )?;
    assert_eq!(
        caps, recovered_caps,
        "readiness is not a wire/capability schema change"
    );
    peer.head.store(true, Ordering::SeqCst);
    let queued = std::time::Instant::now();
    let host = &fixture.host;
    let (a, b, c, d, e, f, g, h) = tokio::join!(
        attach(host, &device, 101, &public),
        attach(host, &device, 101, &public),
        attach(host, &device, 101, &public),
        attach(host, &device, 101, &public),
        attach(host, &device, 101, &public),
        attach(host, &device, 101, &public),
        attach(host, &device, 101, &public),
        attach(host, &device, 101, &public),
    );
    let handles = [a?, b?, c?, d?, e?, f?, g?, h?];
    let converged = page(host, &device, &public, "b").await?;
    assert_eq!(
        converged["messages"]
            .as_array()
            .context("native messages")?
            .len(),
        1,
        "overlapping observations must share one idempotent writer projection"
    );
    let convergence = queued.elapsed();
    assert!(
        convergence < Duration::from_secs(5),
        "many-shard overlapping attach/head: {convergence:?}"
    );
    // Detect a new fault after Ready through an overlapping validator. The
    // shared fault generation must revoke cached-catalog readiness immediately,
    // not wait for the next five-minute sweep; cleanup remains available.
    host.action(
        &device,
        101,
        HostAction::SessionDetach {
            attachment_id: handles[0].clone(),
        },
    )
    .await?;
    std::fs::write(bad, b"injected late authority failure")?;
    assert_eq!(
        host.action(
            &device,
            101,
            HostAction::SessionAttach {
                session_id: public.clone()
            }
        )
        .await?
        .0,
        503
    );
    assert_eq!(
        host.action(&device, 101, HostAction::SessionsList).await?.0,
        503
    );
    assert_eq!(
        host.action(
            &device,
            101,
            HostAction::SessionRead {
                attachment_id: handles[1].clone()
            }
        )
        .await?
        .0,
        503
    );
    for id in handles.into_iter().skip(1) {
        assert_eq!(
            host.action(
                &device,
                101,
                HostAction::SessionDetach { attachment_id: id }
            )
            .await?
            .0,
            200
        );
    }
    std::fs::write(bad, bytes)?;
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let (status, list) = host.action(&device, 101, HostAction::SessionsList).await?;
            if status == 200
                && list["sessions"]
                    .as_array()
                    .is_some_and(|rows| rows.len() == 22)
            {
                break Ok::<_, anyhow::Error>(());
            }
            assert_eq!(status, 503);
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    })
    .await??;
    sampling.store(false, Ordering::SeqCst);
    let (threads, fds) = sampler.await??;
    // Includes Tokio, TLS, sockets, catalog and the existing single writer, not
    // only reader slots. This low-thread fixture does not change real settings.
    assert!(threads <= 24, "process thread peak {threads}");
    assert!(fds <= 64, "process numeric fd peak {fds}");
    for (path, bytes) in &fixture.retained_shards {
        assert_eq!(&std::fs::read(path)?, bytes);
    }
    assert_eq!(std::fs::read(&fixture.source)?, fixture.native_bytes);
    assert!(!fixture.user.join(".local").exists());
    assert_eq!(peer.leases.load(Ordering::SeqCst), 0);
    assert_eq!(peer.writes.load(Ordering::SeqCst), 0);
    eprintln!(
        "synthetic resource/recovery: {SHARDS} extra global shards, 8 overlapping validators, threads_peak={threads}, numeric_fds_peak={fds}, attach/head={convergence:?}"
    );
    fixture.host.stop().await?;
    Ok(())
}

#[tokio::test]
async fn synthetic_3821_header_discovery_yields_to_queued_native_head() -> Result<()> {
    const SOURCES: usize = 3821;
    let mut fixture = Fixture::start().await.context("scale fixture startup")?;
    let peer = Peer::start(&fixture)?;
    let device = fixture.host.enroll(&fixture.host.qr, 101).await?;
    let public = oqto_history::oqto_log::store::platform_id_for_external_id(NATIVE);
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let (_, list) = fixture
                .host
                .action(&device, 101, HostAction::SessionsList)
                .await?;
            if list["sessions"]
                .as_array()
                .is_some_and(|rows| rows.len() == 22)
            {
                break Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .context("scale initial 22-row catalog")??;
    attach(&fixture.host, &device, 101, &public)
        .await
        .context("scale initial attach")?;
    page(&fixture.host, &device, &public, "a")
        .await
        .context("scale initial head")?;
    let root = fixture.user.join(".pi/agent/sessions");
    for n in 0..SOURCES {
        let work = fixture.host.work.join(format!("scale-{}", n % 8));
        std::fs::create_dir_all(&work)?;
        source_file(&root, &work, &format!("synthetic-scale-{n}"), "")?;
    }
    let cold = std::time::Instant::now();
    fixture.restart().await.context("scale cold restart")?;
    // Queue the head AFTER at least one new identity commits: this exercises
    // an in-progress pass, not only a nudge that wins the startup select race.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (_, list) = fixture
                .host
                .action(&device, 101, HostAction::SessionsList)
                .await?;
            if list["sessions"]
                .as_array()
                .is_some_and(|rows| rows.len() > 22 && rows.len() < SOURCES + 22)
            {
                break Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .context("scale initial discovery progress")??;
    peer.head.store(true, Ordering::SeqCst);
    let queued = std::time::Instant::now();
    attach(&fixture.host, &device, 101, &public)
        .await
        .context("scale queued attach")?;
    page(&fixture.host, &device, &public, "b")
        .await
        .context("scale queued head")?;
    let convergence = queued.elapsed();
    assert!(
        convergence < Duration::from_secs(5),
        "queued convergence: {convergence:?}"
    );
    let (_, partial) = fixture
        .host
        .action(&device, 101, HostAction::SessionsList)
        .await?;
    let partial_count = partial["sessions"]
        .as_array()
        .context("partial catalog")?
        .len();
    assert!(
        partial_count < SOURCES + 22,
        "head must converge before the discovery pass finishes"
    );
    let mut latest_status = 0;
    let mut latest_count = 0;
    let completion = tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let (status, list) = fixture
                .host
                .action(&device, 101, HostAction::SessionsList)
                .await?;
            latest_status = status;
            latest_count = list["sessions"].as_array().map_or(0, Vec::len);
            if status == 200
                && list["sessions"]
                    .as_array()
                    .is_some_and(|rows| rows.len() == SOURCES + 22)
            {
                break Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await;
    eprintln!(
        "catalog completion diagnostic: status={latest_status}, rows={latest_count}, elapsed={:?}",
        cold.elapsed()
    );
    completion.context("native catalog completion deadline")??;
    eprintln!(
        "synthetic scale: {SOURCES} fresh headers / 8 workspaces, cold={:?}, queued attach/head={convergence:?}, partial catalog={partial_count}",
        cold.elapsed()
    );
    for (path, bytes) in &fixture.retained_shards {
        assert_eq!(&std::fs::read(path)?, bytes);
    }
    assert_eq!(std::fs::read(&fixture.source)?, fixture.native_bytes);
    assert!(!fixture.user.join(".local").exists());
    fixture.host.stop().await?;
    Ok(())
}

#[path = "split_source/large_history.rs"]
mod large_history;

#[tokio::test]
async fn session_terminal_uses_authoritative_cwd_reuses_owned_pty_and_requires_both_grants()
-> Result<()> {
    let fixture = Fixture::start_with_options(0, false, true).await?;
    fixture.primary().await?;
    let host = &fixture.host;
    let limited = host.enroll(&host.qr, 101).await?;
    let public = oqto_history::oqto_log::store::platform_id_for_external_id(NATIVE);
    let open = HostAction::SessionTerminalOpen {
        session_id: public.clone(),
        rows: 24,
        cols: 80,
    };
    assert_eq!(host.action(&limited, 101, open.clone()).await?.0, 403);
    let ticket = host
        .owner(&[
            "pair",
            "--sessions",
            "observe",
            "--full-user-terminal",
            "--output",
            "json",
        ])
        .await?;
    let ticket = PairingQr::parse(&serde_json::to_vec(&ticket)?, now()?)?;
    let device = host.enroll(&ticket, 102).await?;
    let (status, reply) = host.action(&device, 102, open.clone()).await?;
    assert_eq!(status, 200, "{reply}");
    assert_eq!(reply["session_id"], public);
    let id = reply["terminal_id"]
        .as_str()
        .context("terminal identity")?
        .to_owned();
    let (status, reused) = host.action(&device, 102, open).await?;
    assert_eq!(status, 200);
    assert_eq!(reused["terminal_id"], id);
    assert_eq!(reused["reattached"], true);
    assert_eq!(
        host.action(
            &limited,
            101,
            HostAction::TerminalWrite {
                terminal_id: id.clone(),
                data_base64: STANDARD.encode(b"not sent")
            }
        )
        .await?
        .0,
        403
    );
    assert_eq!(
        host.action(
            &device,
            102,
            HostAction::TerminalWrite {
                terminal_id: id.clone(),
                data_base64: STANDARD.encode(b"printf '\\nSYNTHETIC_CWD='; pwd\r")
            }
        )
        .await?
        .0,
        200
    );
    let mut output = Vec::new();
    let mut after = 0;
    for _ in 0..100 {
        let (status, page) = host
            .action(
                &device,
                102,
                HostAction::TerminalRead {
                    terminal_id: id.clone(),
                    after,
                },
            )
            .await?;
        assert_eq!(status, 200);
        output.extend(STANDARD.decode(page["data_base64"].as_str().context("terminal output")?)?);
        after = page["next"].as_u64().context("cursor")?;
        if String::from_utf8_lossy(&output)
            .contains(&format!("SYNTHETIC_CWD={}", host.work.display()))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        String::from_utf8_lossy(&output)
            .contains(&format!("SYNTHETIC_CWD={}", host.work.display()))
    );
    assert_eq!(
        host.action(&device, 102, HostAction::TerminalClose { terminal_id: id })
            .await?
            .0,
        200
    );
    fixture.host.stop().await?;
    Ok(())
}

#[tokio::test]
async fn full_startup_split_home_cold_catalog_native_graph_cohost_restart_and_zero_pi_exec()
-> Result<()> {
    let mut fixture = Fixture::start().await?;
    let peer = Peer::start(&fixture)?;
    fixture.primary().await?;
    let host = &fixture.host;
    let device = host.enroll(&host.qr, 101).await?;
    let (_, caps) = host.action(&device, 101, HostAction::Capabilities).await?;
    assert_eq!(caps["roots"], serde_json::json!([]));
    assert_eq!(caps["full_user_terminal_granted"], false);
    assert_eq!(caps["terminal_supported"], false);
    assert_eq!(caps["session_control_granted"], true);
    for action in [
        HostAction::FilesList {
            root_id: "invented".into(),
            path: "".into(),
        },
        HostAction::TerminalOpen {
            cwd: host.work.to_string_lossy().into(),
            rows: 24,
            cols: 80,
        },
    ] {
        assert_eq!(host.action(&device, 101, action).await?.0, 403);
    }
    let public = oqto_history::oqto_log::store::platform_id_for_external_id(NATIVE);
    let fresh = oqto_history::oqto_log::store::platform_id_for_external_id(FRESH);
    assert_ne!(public, NATIVE);
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let (status, list) = host.action(&device, 101, HostAction::SessionsList).await?;
            if status == 200
                && list["sessions"]
                    .as_array()
                    .is_some_and(|rows| rows.len() == 22)
                && list.to_string().contains(&fresh)
                && list.to_string().contains(
                    &oqto_history::oqto_log::store::platform_id_for_external_id("default-15"),
                )
            {
                assert!(list.to_string().contains(&public));
                assert!(!list.to_string().contains(
                    &oqto_history::oqto_log::store::platform_id_for_external_id("outside-native")
                ));
                break Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await??;
    let rows = oqto_history::oqto_log::ops::list_sessions(&host.home, None).await?;
    assert_eq!(
        rows.len(),
        22,
        "fixture rows: {:?}",
        rows.iter().map(|row| &row.external_id).collect::<Vec<_>>()
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row
                .external_id
                .as_deref()
                .is_some_and(|id| id.starts_with("default-")))
            .count(),
        16
    );
    let owner = rows
        .iter()
        .find(|row| row.platform_id == public)
        .context("public row")?
        .user_id
        .clone();
    assert_eq!(
        rows.iter()
            .find(|row| row.platform_id == fresh)
            .context("fresh row")?
            .messages,
        0
    );
    assert_ne!(
        host.action(
            &device,
            101,
            HostAction::SessionAttach {
                session_id: fresh.clone()
            }
        )
        .await?
        .0,
        200
    );
    // A retained catalog identity without a native file/peer remains visible,
    // but attachment fails closed rather than migrating or repairing its store.
    for session in ["oqto-retained-april-child", "oqto-retained-july-child"] {
        assert_ne!(
            host.action(
                &device,
                101,
                HostAction::SessionAttach {
                    session_id: session.into(),
                }
            )
            .await?
            .0,
            200
        );
    }
    let id = attach(host, &device, 101, &public).await?;
    assert_eq!(peer.leases.load(Ordering::SeqCst), 0);
    let first = page(host, &device, &public, "a").await?;
    assert_eq!(first["messages"].as_array().context("messages")?.len(), 1);
    // An equal stat cursor cannot suppress fresh socket-head reconciliation.
    let metadata = std::fs::metadata(&fixture.source)?;
    oqto_history::oqto_log::index::upsert_ingest_cursor(
        &host.home,
        NATIVE,
        oqto_history::oqto_log::index::IngestCursor {
            file_size: metadata.len() as i64,
            file_mtime_ms: metadata.modified()?.duration_since(UNIX_EPOCH)?.as_millis() as i64,
        },
    )
    .await?;
    peer.head.store(true, Ordering::SeqCst);
    assert_eq!(
        host.action(
            &device,
            101,
            HostAction::SessionRead {
                attachment_id: id.clone()
            }
        )
        .await?
        .0,
        200
    );
    let second = page(host, &device, &public, "b").await?;
    assert_ne!(first["messages"][0]["id"], second["messages"][0]["id"]);
    assert!(
        second["native_graph"]["revision"].as_u64() > first["native_graph"]["revision"].as_u64()
    );
    let observe = host
        .owner(&["pair", "--sessions", "observe", "--output", "json"])
        .await?;
    let observe = PairingQr::parse(&serde_json::to_vec(&observe)?, now()?)?;
    let observer = host.enroll(&observe, 102).await?;
    let observe_id = attach(host, &observer, 102, &public).await?;
    assert_eq!(
        host.action(
            &observer,
            102,
            HostAction::SessionRequestControl {
                attachment_id: observe_id.clone()
            }
        )
        .await?
        .0,
        403
    );
    assert_eq!(
        host.action(
            &observer,
            102,
            HostAction::SessionDetach {
                attachment_id: observe_id
            }
        )
        .await?
        .0,
        200
    );
    assert_eq!(
        host.action(
            &device,
            101,
            HostAction::SessionRequestControl {
                attachment_id: id.clone()
            }
        )
        .await?
        .0,
        200
    );
    assert_eq!(peer.leases.load(Ordering::SeqCst), 1);
    peer.uncertain.store(true, Ordering::SeqCst);
    assert_ne!(
        host.action(
            &device,
            101,
            HostAction::SessionInput {
                attachment_id: id.clone(),
                input: SessionInput::Prompt {
                    message: "one uncertain input".into()
                }
            }
        )
        .await?
        .0,
        200
    );
    assert_ne!(
        host.action(
            &device,
            101,
            HostAction::SessionInput {
                attachment_id: id,
                input: SessionInput::Abort
            }
        )
        .await?
        .0,
        200
    );
    assert_eq!(peer.writes.load(Ordering::SeqCst), 1);
    let id = attach(host, &device, 101, &public).await?;
    assert_eq!(
        host.action(
            &device,
            101,
            HostAction::SessionRequestControl {
                attachment_id: id.clone()
            }
        )
        .await?
        .0,
        200
    );
    assert_eq!(host.owner(&["revoke", &device]).await?["revoked"], true);
    assert_eq!(
        host.action(&device, 101, HostAction::SessionRead { attachment_id: id })
            .await?
            .0,
        401
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        while peer.active_leases.load(Ordering::SeqCst) != 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    fixture.restart().await?;
    fixture.primary().await?;
    let host = &fixture.host;
    // Same Device, TLS/host identity and canonical Session survive restart;
    // revoked authority does not return, and the terminal peer is still alive.
    assert_eq!(
        host.action(&device, 101, HostAction::Capabilities).await?.0,
        401
    );
    let id = attach(host, &observer, 102, &public).await?;
    assert_eq!(
        host.action(
            &observer,
            102,
            HostAction::SessionDetach { attachment_id: id }
        )
        .await?
        .0,
        200
    );
    let rows = oqto_history::oqto_log::ops::list_sessions(&host.home, None).await?;
    assert_eq!(
        rows.iter()
            .find(|row| row.platform_id == public)
            .context("same row")?
            .user_id,
        owner
    );
    for (path, bytes) in &fixture.retained_shards {
        // All list/attach/history/reload/discovery/restart operations preserve
        // the complete old schema, IDs, owner, history and provenance bytes.
        assert_eq!(&std::fs::read(path)?, bytes);
        assert!(!path.with_extension("sqlite-wal").exists());
        assert!(!path.with_extension("sqlite-shm").exists());
    }
    assert_eq!(std::fs::read(&fixture.source)?, fixture.native_bytes);
    assert_eq!(
        std::fs::read(fixture.root.join("config.toml"))?,
        fixture.config_bytes
    );
    assert_eq!(
        [
            std::fs::read(fixture.root.join("mtls.pem"))?,
            std::fs::read(fixture.root.join("mtls.key"))?
        ],
        fixture.transport_bytes
    );
    assert_eq!(
        std::fs::read(fixture.user.join(".pi/agent/profile-sentinel"))?,
        b"private synthetic profile: unchanged"
    );
    assert!(!fixture.user.join(".local").exists());
    assert!(!host.home.join("pi-invocations").exists());
    assert_eq!(peer.writes.load(Ordering::SeqCst), 1);
    fixture.host.stop().await?;
    Ok(())
}
