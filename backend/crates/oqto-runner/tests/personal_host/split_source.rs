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
}
impl Fixture {
    async fn start() -> Result<Self> {
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
        let config_bytes = format!("[local]\nsingle_user=true\nterminal_enabled=false\nworkspace_dir={}\n[runner]\nrunner_id=\"split-fixture\"\n[pi]\nexecutable={}\n[unknown_future]\npreserve=\"opaque synthetic value\"\n", serde_json::to_string(&work)?, serde_json::to_string(&sentinel)?).into_bytes();
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
        let mut child = command(&home)
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
        self.host.child = command(&self.host.home)
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
        let (source, cwd) = (fixture.source.clone(), fixture.host.work.clone());
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
                    .is_some_and(|rows| rows.len() == 18)
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
        18,
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
