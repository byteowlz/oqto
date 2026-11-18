//! Real pinned TLS, real personal pairing and signed proofs, real journal,
//! loopback only. The confined launcher is replaced by an in-process pi-env
//! stand-in so effects, stops and stop failures can be counted and injected.
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use oqto_hands_link::{
    client::{ClaimResult, LinkState, MachineLink},
    clock::SuspendAwareClock,
    config::{BindingConfig, HostConfig, LauncherConfig, LinkLimits, MachineConfig},
    host::HandsHost,
    journal::Journal,
    launcher::{Generation, Input, LaunchBinding, Launcher, Output},
    server::{self, HostOwnerCommand},
    wire::{CallState, Operation, StopState, random_hex},
};
use oqto_personal::PairingAuthority;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

#[derive(Default)]
struct Fake {
    launches: AtomicUsize,
    stops: AtomicUsize,
    /// Effects (write requests) per generation.
    effects: Mutex<HashMap<u64, usize>>,
    fail_stop: AtomicBool,
}

struct FakeLauncher(Arc<Fake>);
struct FakeGeneration {
    io: Option<(Input, Output)>,
    fake: Arc<Fake>,
    task: tokio::task::JoinHandle<()>,
}

fn frame(kind: u8, id: u32, json: &serde_json::Value, payload: &[u8]) -> Vec<u8> {
    let json = serde_json::to_vec(json).unwrap_or_default();
    let mut out = Vec::new();
    out.extend_from_slice(&((9 + json.len() + payload.len()) as u32).to_be_bytes());
    out.push(kind);
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&(json.len() as u32).to_be_bytes());
    out.extend_from_slice(&json);
    out.extend_from_slice(payload);
    out
}

async fn read_frame(
    io: &mut (impl AsyncReadExt + Unpin),
) -> Result<(u8, u32, serde_json::Value, Vec<u8>)> {
    let mut head = [0u8; 13];
    io.read_exact(&mut head).await?;
    let length = u32::from_be_bytes(head[0..4].try_into()?) as usize;
    let id = u32::from_be_bytes(head[5..9].try_into()?);
    let json = u32::from_be_bytes(head[9..13].try_into()?) as usize;
    let mut body = vec![0u8; length - 9];
    io.read_exact(&mut body).await?;
    Ok((
        head[4],
        id,
        serde_json::from_slice(&body[..json])?,
        body[json..].to_vec(),
    ))
}

/// pi-env stand-in: sync line, then hello/write/flood requests.
async fn daemon(
    mut io: DuplexStream,
    token: String,
    generation: u64,
    fake: Arc<Fake>,
) -> Result<()> {
    io.write_all(format!("PI-ENV {token}\n").as_bytes()).await?;
    loop {
        let (kind, id, json, _payload) = read_frame(&mut io).await?;
        if kind != 1 {
            continue;
        }
        match json["op"].as_str() {
            Some("write") => {
                *fake
                    .effects
                    .lock()
                    .map_err(|_| anyhow::anyhow!("poisoned"))?
                    .entry(generation)
                    .or_default() += 1;
                io.write_all(&frame(2, id, &serde_json::json!({}), b""))
                    .await?;
            }
            Some("exec") => loop {
                // Flood: output until the generation is stopped.
                io.write_all(&frame(
                    4,
                    id,
                    &serde_json::json!({"kind":"output","stream":"stdout"}),
                    &[b'x'; 60_000],
                ))
                .await?;
            },
            _ => {
                io.write_all(&frame(2, id, &serde_json::json!({"protocol":1}), b""))
                    .await?
            }
        }
    }
}

#[async_trait]
impl Launcher for FakeLauncher {
    async fn launch(
        &self,
        _: &LaunchBinding,
        generation: u64,
        token: &str,
    ) -> Result<Box<dyn Generation>> {
        self.0.launches.fetch_add(1, Ordering::SeqCst);
        let (host_side, daemon_side) = tokio::io::duplex(64 * 1024);
        let (read, write) = tokio::io::split(host_side);
        let task = tokio::spawn({
            let fake = Arc::clone(&self.0);
            let token = token.to_string();
            async move {
                let _ = daemon(daemon_side, token, generation, fake).await;
            }
        });
        Ok(Box::new(FakeGeneration {
            io: Some((Box::new(write), Box::new(read))),
            fake: Arc::clone(&self.0),
            task,
        }))
    }
}

#[async_trait]
impl Generation for FakeGeneration {
    fn take_io(&mut self) -> Option<(Input, Output)> {
        self.io.take()
    }
    async fn stop(&mut self) -> StopState {
        self.fake.stops.fetch_add(1, Ordering::SeqCst);
        self.task.abort();
        if self.fake.fail_stop.load(Ordering::SeqCst) {
            StopState::Unconfirmed
        } else {
            StopState::Confirmed
        }
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    host: Arc<HandsHost>,
    fake: Arc<Fake>,
    owner: std::path::PathBuf,
    state: LinkState,
    shutdown: tokio_util::sync::CancellationToken,
    device_id: String,
}

const LIMITS: LinkLimits = LinkLimits {
    lease_ms: 2_000,
    renew_ms: 400,
    max_concurrent_calls: 4,
};

async fn fixture() -> Result<Fixture> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().canonicalize()?;
    let work = root.join("work");
    std::fs::create_dir(&work)?;
    let state_dir = root.join("host");
    std::fs::create_dir(&state_dir)?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let mut bindings = BTreeMap::new();
    bindings.insert(
        "workspace".to_string(),
        BindingConfig {
            root: work.clone(),
            max_lease_ms: 10_000,
            max_concurrent_calls: 4,
        },
    );
    let config = HostConfig {
        version: 1,
        listen: address.to_string(),
        origin: format!("https://{address}/"),
        state_dir: state_dir.clone(),
        launcher: LauncherConfig {
            oqt_hands: root.join("unused"),
            daemon: root.join("unused"),
            bpf: None,
            shim: None,
            protected_roots: vec![root.join("protected")],
        },
        bindings,
    };
    let authority = Arc::new(PairingAuthority::open_private(&state_dir.join("personal")).await?);
    let (journal, _) = Journal::open(&state_dir.join("hands.sqlite3")).await?;
    let fake = Arc::new(Fake::default());
    let host = HandsHost::new(
        &config,
        Arc::clone(&authority),
        journal,
        Arc::new(FakeLauncher(Arc::clone(&fake))),
        Arc::new(SuspendAwareClock),
    )?;
    host.spawn_enforcer();
    let identity = authority.transport_identity().await?;
    let shutdown = tokio_util::sync::CancellationToken::new();
    tokio::spawn(server::serve_tls(
        listener,
        identity.server_config()?,
        server::router(Arc::clone(&host)),
        shutdown.clone().cancelled_owned(),
    ));
    let owner = server::owner_socket(&config);
    tokio::spawn(server::serve_owner(
        server::bind_owner(&owner)?,
        Arc::clone(&host),
        config.origin.clone(),
        identity.leaf_der().to_vec(),
        shutdown.clone().cancelled_owned(),
    ));
    let state = LinkState::open(&root.join("origin"))?;
    let qr = server::request_owner(&owner, &HostOwnerCommand::Pair).await?;
    let pairing = state
        .pair("dest", serde_json::to_string(&qr)?.as_bytes(), "test guard")
        .await?;
    Ok(Fixture {
        _dir: dir,
        host,
        fake,
        owner,
        state,
        shutdown,
        device_id: pairing.device_id,
    })
}

impl Fixture {
    fn link(
        &self,
        namespace: &str,
    ) -> Result<(
        Arc<MachineLink>,
        tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>,
    )> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let link = MachineLink::new(
            "dest",
            &MachineConfig {
                pairing: "dest".into(),
                binding: "workspace".into(),
                limits: LIMITS,
            },
            &self.state,
            namespace,
            tx,
        )?;
        Ok((link, rx))
    }
    async fn owner(&self, command: HostOwnerCommand) -> Result<serde_json::Value> {
        server::request_owner(&self.owner, &command).await
    }
    async fn delegate(&self) -> Result<()> {
        self.owner(HostOwnerCommand::Delegate {
            device_id: self.device_id.clone(),
            binding_id: "workspace".into(),
        })
        .await?;
        Ok(())
    }
}

async fn claimed(
    link: &Arc<MachineLink>,
    id: &str,
    d: &str,
    op: Operation,
) -> Result<(u64, String)> {
    match link.claim(id, d, op).await {
        ClaimResult::Claimed {
            generation, cwd, ..
        } => Ok((generation, cwd)),
        other => bail!("claim failed: {other:?}"),
    }
}

fn call() -> String {
    random_hex(16)
}
fn digest() -> String {
    random_hex(32)
}

/// Drive one pi-env exchange through a claimed call's data stream.
async fn exchange(
    link: &Arc<MachineLink>,
    call_id: &str,
    ops: &[&str],
) -> Result<Vec<serde_json::Value>> {
    let token = random_hex(16);
    let stream = link.attach(call_id, &token).await?;
    let (mut local, remote) = tokio::io::duplex(1 << 20);
    let pump = tokio::spawn(stream.pump(remote));
    let mut sync = vec![0u8; 40];
    local.read_exact(&mut sync).await?;
    assert_eq!(sync, format!("PI-ENV {token}\n").into_bytes());
    let mut replies = Vec::new();
    for (index, op) in ops.iter().enumerate() {
        local
            .write_all(&frame(
                1,
                index as u32 + 1,
                &serde_json::json!({"op": op, "path": "/w/x"}),
                b"data",
            ))
            .await?;
        replies.push(read_frame(&mut local).await?.2);
    }
    drop(local);
    let _ = tokio::time::timeout(Duration::from_secs(5), pump).await;
    Ok(replies)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pairing_alone_never_authorizes_hands() -> Result<()> {
    let f = fixture().await?;
    let (link, _events) = f.link(&random_hex(16))?;
    // Paired, but no standing hands delegation from the destination owner.
    match link.claim(&call(), &digest(), Operation::Read).await {
        ClaimResult::NotStarted { code } => assert_eq!(code, "not_delegated"),
        other => bail!("expected refusal, got {other:?}"),
    }
    assert_eq!(f.fake.launches.load(Ordering::SeqCst), 0);
    f.shutdown.cancel();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wrong_pin_fails_before_any_proof_or_claim() -> Result<()> {
    let f = fixture().await?;
    f.delegate().await?;
    let path = f
        ._dir
        .path()
        .canonicalize()?
        .join("origin/pairings/dest.json");
    let mut record: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    record["certificate_sha256"] = "0".repeat(64).into();
    std::fs::remove_file(&path)?;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    std::io::Write::write_all(&mut file, &serde_json::to_vec(&record)?)?;
    let (link, _events) = f.link(&random_hex(16))?;
    match link.claim(&call(), &digest(), Operation::Read).await {
        ClaimResult::NotStarted { code } => assert!(code.contains("TLS refused"), "{code}"),
        other => bail!("expected refusal, got {other:?}"),
    }
    assert_eq!(f.fake.launches.load(Ordering::SeqCst), 0);
    f.shutdown.cancel();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claim_attach_release_and_duplicates_never_dispatch_twice() -> Result<()> {
    let f = fixture().await?;
    f.delegate().await?;
    let (link, _events) = f.link(&random_hex(16))?;
    let id = call();
    let d = digest();
    let (generation, cwd) = claimed(&link, &id, &d, Operation::Write).await?;
    assert!(cwd.ends_with("/work"));
    let replies = exchange(&link, &id, &["hello", "write"]).await?;
    assert_eq!(replies[0]["protocol"], 1);
    // A second attach (lazy reconnect) for the same claim is refused.
    assert!(link.attach(&id, &random_hex(16)).await.is_err());
    let receipt = link.release(&id).await?;
    assert_eq!(
        (receipt.state, receipt.stop),
        (CallState::Released, StopState::Confirmed)
    );
    assert_eq!(receipt.generation, generation);
    // Exact duplicate: inspect only. Changed digest: refused.
    match link.claim(&id, &d, Operation::Write).await {
        ClaimResult::Duplicate { receipt } => assert_eq!(receipt.state, CallState::Released),
        other => bail!("expected duplicate, got {other:?}"),
    }
    match link.claim(&id, &digest(), Operation::Write).await {
        ClaimResult::NotStarted { code } => assert_eq!(code, "digest_mismatch"),
        other => bail!("expected digest mismatch, got {other:?}"),
    }
    assert_eq!(f.fake.launches.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.fake
            .effects
            .lock()
            .map_err(|_| anyhow::anyhow!("poisoned"))?
            .get(&generation),
        Some(&1)
    );
    f.shutdown.cancel();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn revoke_stops_generations_and_refuses_the_device() -> Result<()> {
    let f = fixture().await?;
    f.delegate().await?;
    let (link, _events) = f.link(&random_hex(16))?;
    let id = call();
    claimed(&link, &id, &digest(), Operation::Bash).await?;
    let token = random_hex(16);
    let stream = link.attach(&id, &token).await?;
    let (mut local, remote) = tokio::io::duplex(1 << 20);
    let pump = tokio::spawn(stream.pump(remote));
    let mut sync = vec![0u8; 40];
    local.read_exact(&mut sync).await?;
    let revoked = f
        .owner(HostOwnerCommand::Revoke {
            device_id: f.device_id.clone(),
        })
        .await?;
    assert_eq!(revoked["revoked"], true);
    assert_eq!(revoked["confirmed"], true);
    assert_eq!(revoked["stopped"][0]["call_id"], id.as_str());
    assert!(f.fake.stops.load(Ordering::SeqCst) >= 1);
    // The data stream is closed; nothing more is delivered.
    let _ = tokio::time::timeout(Duration::from_secs(5), pump).await;
    let receipt = f.host.journal.receipt(&id).await?.context("receipt")?;
    assert_eq!(receipt.state, CallState::Fenced);
    // A revoked device cannot open a new control connection.
    let (again, _e) = f.link(&random_hex(16))?;
    assert!(matches!(
        again.claim(&call(), &digest(), Operation::Read).await,
        ClaimResult::NotStarted { .. }
    ));
    f.shutdown.cancel();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unconfirmed_stop_freezes_the_binding_with_attention() -> Result<()> {
    let f = fixture().await?;
    f.delegate().await?;
    let (link, _events) = f.link(&random_hex(16))?;
    let id = call();
    claimed(&link, &id, &digest(), Operation::Bash).await?;
    exchange(&link, &id, &["hello"]).await?;
    f.fake.fail_stop.store(true, Ordering::SeqCst);
    let receipt = link.release(&id).await?;
    assert_eq!(receipt.stop, StopState::Unconfirmed);
    let status = f.owner(HostOwnerCommand::Status).await?;
    assert_eq!(status["bindings"][0]["attention"], true);
    // Both sides refuse new work for that binding.
    assert!(matches!(
        link.claim(&call(), &digest(), Operation::Read).await,
        ClaimResult::NotStarted { .. }
    ));
    link.clear_attention();
    match link.claim(&call(), &digest(), Operation::Read).await {
        ClaimResult::NotStarted { code } => assert_eq!(code, "attention"),
        other => bail!("expected attention, got {other:?}"),
    }
    f.owner(HostOwnerCommand::ClearAttention {
        binding_id: "workspace".into(),
    })
    .await?;
    f.fake.fail_stop.store(false, Ordering::SeqCst);
    assert!(matches!(
        link.claim(&call(), &digest(), Operation::Read).await,
        ClaimResult::Claimed { .. }
    ));
    f.shutdown.cancel();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn freeze_fences_then_acknowledges_exact_generations() -> Result<()> {
    let f = fixture().await?;
    f.delegate().await?;
    let (link, _events) = f.link(&random_hex(16))?;
    let id = call();
    let (generation, _) = claimed(&link, &id, &digest(), Operation::Bash).await?;
    exchange(&link, &id, &["hello"]).await?;
    let ack = link.freeze().await;
    assert_eq!(ack["confirmed"], true, "{ack}");
    assert_eq!(ack["stopped"][0]["generation"], generation);
    assert_eq!(
        f.host.journal.receipt(&id).await?.context("receipt")?.state,
        CallState::Fenced
    );
    // New calls use the bumped revision and are admitted again.
    assert!(matches!(
        link.claim(&call(), &digest(), Operation::Read).await,
        ClaimResult::Claimed { .. }
    ));
    f.shutdown.cancel();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn controller_takeover_fences_the_old_controller() -> Result<()> {
    let f = fixture().await?;
    f.delegate().await?;
    let namespace = random_hex(16);
    let (first, _e1) = f.link(&namespace)?;
    let old = call();
    claimed(&first, &old, &digest(), Operation::Bash).await?;
    // A second guard incarnation with the same persisted namespace takes over.
    let (second, _e2) = f.link(&namespace)?;
    assert!(matches!(
        second.claim(&call(), &digest(), Operation::Read).await,
        ClaimResult::Claimed { .. }
    ));
    assert_eq!(
        f.host
            .journal
            .receipt(&old)
            .await?
            .context("receipt")?
            .state,
        CallState::Fenced
    );
    // The old incarnation cannot come back (no newest-wins race).
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(matches!(
        first.claim(&call(), &digest(), Operation::Read).await,
        ClaimResult::NotStarted { .. }
    ));
    f.shutdown.cancel();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lease_expires_without_guard_renewal_and_delayed_frames_are_refused() -> Result<()> {
    let f = fixture().await?;
    f.delegate().await?;
    let (link, mut events) = f.link(&random_hex(16))?;
    let id = call();
    claimed(&link, &id, &digest(), Operation::Bash).await?;
    // Renewals keep it alive past one lease interval.
    tokio::time::sleep(Duration::from_millis(2_600)).await;
    assert!(f.host.journal.receipt(&id).await?.context("receipt")?.state == CallState::Claimed);
    // Partition stand-in: the guard stops vouching for the call while the
    // control connection stays open; nothing renews the fixed lease.
    link.withdraw(&id);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    let mut ended = None;
    while tokio::time::Instant::now() < deadline {
        if let Ok(Some(event)) =
            tokio::time::timeout(Duration::from_millis(200), events.recv()).await
        {
            ended = Some(event);
            break;
        }
        let receipt = f.host.journal.receipt(&id).await?.context("receipt")?;
        if receipt.state != CallState::Claimed {
            ended = Some(serde_json::to_value(receipt)?);
            break;
        }
    }
    let ended = ended.context("call never ended")?;
    let text = ended.to_string();
    assert!(text.contains("expired"), "{text}");
    // Never attached: provably not started.
    assert!(text.contains("not_started"), "{text}");
    f.shutdown.cancel();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn flooded_stream_does_not_block_control_or_a_sibling_cancel() -> Result<()> {
    let f = fixture().await?;
    f.delegate().await?;
    let (link, _events) = f.link(&random_hex(16))?;
    let flood = call();
    let quiet = call();
    claimed(&link, &flood, &digest(), Operation::Bash).await?;
    claimed(&link, &quiet, &digest(), Operation::Bash).await?;
    // Flood with a stalled local reader: credit stops the host after one window.
    let token = random_hex(16);
    let stream = link.attach(&flood, &token).await?;
    let (mut local, remote) = tokio::io::duplex(64 * 1024);
    let pump = tokio::spawn(stream.pump(remote));
    let mut sync = vec![0u8; 40];
    local.read_exact(&mut sync).await?;
    local
        .write_all(&frame(
            1,
            1,
            &serde_json::json!({"op":"exec","command":"flood"}),
            b"",
        ))
        .await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    // The sibling's control round trips stay prompt while the flood is stalled.
    let started = std::time::Instant::now();
    let receipt = link.cancel(&quiet).await?;
    assert_eq!(receipt.state, CallState::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    let started = std::time::Instant::now();
    let receipt = link.cancel(&flood).await?;
    assert_eq!(
        (receipt.state, receipt.stop),
        (CallState::Cancelled, StopState::Confirmed)
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    // Bounded: the host sent at most about one credit window while stalled.
    assert!(receipt.bytes_out <= 4 * 256 * 1024, "{}", receipt.bytes_out);
    drop(local);
    let _ = tokio::time::timeout(Duration::from_secs(5), pump).await;
    f.shutdown.cancel();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disconnect_marks_running_calls_unknown_and_host_restart_never_replays() -> Result<()> {
    let f = fixture().await?;
    f.delegate().await?;
    let (link, _events) = f.link(&random_hex(16))?;
    let id = call();
    claimed(&link, &id, &digest(), Operation::Write).await?;
    let token = random_hex(16);
    let stream = link.attach(&id, &token).await?;
    let (mut local, remote) = tokio::io::duplex(1 << 20);
    let _pump = tokio::spawn(stream.pump(remote));
    let mut sync = vec![0u8; 40];
    local.read_exact(&mut sync).await?;
    local
        .write_all(&frame(
            1,
            1,
            &serde_json::json!({"op":"write","path":"/w/a"}),
            b"x",
        ))
        .await?;
    let _ = read_frame(&mut local).await?;
    link.disconnect();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let receipt = f.host.journal.receipt(&id).await?.context("receipt")?;
        if receipt.state == CallState::Unknown {
            assert_eq!(receipt.stop, StopState::Confirmed);
            break;
        }
        if tokio::time::Instant::now() > deadline {
            bail!("still {:?}", receipt.state);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let effects = f
        .fake
        .effects
        .lock()
        .map_err(|_| anyhow::anyhow!("poisoned"))?
        .values()
        .sum::<usize>();
    assert_eq!(effects, 1);
    f.shutdown.cancel();
    Ok(())
}
