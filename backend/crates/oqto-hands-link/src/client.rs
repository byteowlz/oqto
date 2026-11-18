//! Origin side of the hands link: pairing state, one authenticated control
//! connection per machine, and per-call data streams. Owned by the guard;
//! the brain never sees a key, a socket to the destination or this state.
//!
//! One retry owner: nothing here resends a claim, a frame or a call. A lost
//! reply after a claim was sent makes that call unknown.
use crate::{
    config::{MachineConfig, OriginConfig},
    wire::{self, *},
};
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use futures::{SinkExt, StreamExt};
use oqto_personal_wire::{OQTO_PERSONAL_V1, PairingQr, SignedRequest};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    sync::{mpsc, oneshot},
};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{Message, protocol::WebSocketConfig},
};

type Tls = tokio_rustls::client::TlsStream<TcpStream>;
type Socket = WebSocketStream<Tls>;

/// Nonsecret pairing record (state, not config): who the machine is.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pairing {
    pub host_id: String,
    pub origin: String,
    pub certificate_sha256: String,
    pub device_id: String,
}

/// Guard-private link state directory (`$XDG_STATE_HOME/oqt/hands-link`).
#[derive(Clone, Debug)]
pub struct LinkState {
    root: PathBuf,
}

fn private_dir(path: &Path) -> Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.is_dir()
            && meta.mode() & 0o077 == 0
            && meta.uid() == rustix::process::geteuid().as_raw(),
        "{} must be a private owned directory",
        path.display()
    );
    Ok(())
}

fn read_private(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let meta = std::fs::symlink_metadata(path).with_context(|| format!("{}", path.display()))?;
    ensure!(
        meta.is_file()
            && meta.mode() & 0o077 == 0
            && meta.uid() == rustix::process::geteuid().as_raw()
            && meta.len() <= limit,
        "{} must be a private owned file",
        path.display()
    );
    Ok(std::fs::read(path)?)
}

fn write_private(path: &Path, bytes: &[u8], create_new: bool) -> Result<()> {
    let temp = path.with_extension(format!("tmp-{}", random_hex(4)));
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    if create_new && path.exists() {
        let _ = std::fs::remove_file(&temp);
        bail!("{} already exists; refusing to overwrite", path.display());
    }
    std::fs::rename(&temp, path)?;
    Ok(())
}

impl LinkState {
    pub fn open(root: &Path) -> Result<Self> {
        ensure!(root.is_absolute(), "link state must be absolute");
        private_dir(root)?;
        for sub in ["pairings", "keys", "controllers", "fences"] {
            private_dir(&root.join(sub))?;
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
    }
    pub fn pairing(&self, name: &str) -> Result<Pairing> {
        ensure!(
            crate::config::valid_machine_name(name),
            "invalid pairing name"
        );
        let bytes = read_private(
            &self.root.join("pairings").join(format!("{name}.json")),
            4096,
        )?;
        Ok(serde_json::from_slice(&bytes)?)
    }
    fn key(&self, name: &str) -> Result<SigningKey> {
        let bytes = read_private(&self.root.join("keys").join(format!("{name}.key")), 256)?;
        let seed: [u8; 32] = STANDARD
            .decode(String::from_utf8(bytes)?.trim())?
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid device key"))?;
        Ok(SigningKey::from_bytes(&seed))
    }

    /// Controller takeover revision per (machine, namespace), persisted
    /// before it is presented so a crash cannot reuse an epoch.
    fn next_controller(&self, machine: &str, namespace: &str) -> Result<(u64, u64)> {
        let path = self
            .root
            .join("controllers")
            .join(format!("{machine}-{namespace}.json"));
        let previous = match read_private(&path, 1024) {
            Ok(bytes) => serde_json::from_slice::<serde_json::Value>(&bytes)?["epoch"]
                .as_u64()
                .context("invalid controller state")?,
            Err(_) if !path.exists() => 0,
            Err(error) => return Err(error),
        };
        let epoch = previous + 1;
        write_private(
            &path,
            &serde_json::to_vec(&serde_json::json!({"epoch": epoch}))?,
            false,
        )?;
        Ok((previous, epoch))
    }

    /// Origin policy revision per machine (shared by every namespace).
    pub fn policy_revision(&self, machine: &str) -> Result<u64> {
        let path = self.root.join("fences").join(format!("{machine}.json"));
        if !path.exists() {
            return Ok(0);
        }
        serde_json::from_slice::<serde_json::Value>(&read_private(&path, 1024)?)?["revision"]
            .as_u64()
            .context("invalid fence state")
    }
    /// Persist the next revision before anything is sent (revoke step 1).
    pub fn bump_policy(&self, machine: &str) -> Result<u64> {
        let next = self.policy_revision(machine)? + 1;
        let path = self.root.join("fences").join(format!("{machine}.json"));
        write_private(
            &path,
            &serde_json::to_vec(&serde_json::json!({"revision": next}))?,
            false,
        )?;
        Ok(next)
    }

    /// Pair this guard with a hands host from its one-use QR. Writes the
    /// device key and the nonsecret record; never prints either.
    pub async fn pair(&self, name: &str, qr: &[u8], label: &str) -> Result<Pairing> {
        ensure!(
            crate::config::valid_machine_name(name),
            "invalid pairing name"
        );
        let key_path = self.root.join("keys").join(format!("{name}.key"));
        let record_path = self.root.join("pairings").join(format!("{name}.json"));
        ensure!(
            !key_path.exists() && !record_path.exists(),
            "pairing {name} exists; remove it explicitly before re-pairing"
        );
        let now = unix_now()?;
        let qr = PairingQr::parse(qr, now)?;
        let mut seed = [0u8; 32];
        use rand::RngCore;
        rand::rng().fill_bytes(&mut seed);
        let key = SigningKey::from_bytes(&seed);
        let mut enrollment = oqto_personal_wire::Enrollment {
            token: qr.enrollment_token().into(),
            name: label.into(),
            public_key: STANDARD.encode(key.verifying_key().to_bytes()),
            signature: String::new(),
        };
        enrollment.signature = STANDARD.encode(
            key.sign(&enrollment.signing_bytes_for(OQTO_PERSONAL_V1))
                .to_bytes(),
        );
        let tls = oqto_personal_wire::tls::pinned_client_config(qr.certificate_sha256())?;
        let client = reqwest::Client::builder()
            .use_preconfigured_tls(tls)
            .https_only(true)
            .http1_only()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .build()?;
        let response = client
            .post(format!(
                "{}{}",
                qr.origin().trim_end_matches('/'),
                oqto_personal_wire::ENROLL_PATH
            ))
            .json(&enrollment)
            .send()
            .await
            .context("enrollment request failed; the one-use QR may be consumed")?;
        ensure!(response.status().is_success(), "enrollment refused");
        let reply: oqto_personal_wire::EnrollmentReply = response.json().await?;
        ensure!(
            reply.version == 1 && reply.host_id == qr.host_id(),
            "enrollment reply for another host"
        );
        write_private(&key_path, STANDARD.encode(seed).as_bytes(), true)?;
        let pairing = Pairing {
            host_id: reply.host_id,
            origin: qr.origin().into(),
            certificate_sha256: qr.certificate_sha256().into(),
            device_id: reply.device_id,
        };
        write_private(&record_path, &serde_json::to_vec_pretty(&pairing)?, true)?;
        Ok(pairing)
    }
}

pub fn unix_now() -> Result<i64> {
    Ok(i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
    )?)
}

fn sign(key: &SigningKey, device_id: &str, proof: &HandsProof) -> Result<SignedRequest> {
    let mut request = SignedRequest {
        device_id: device_id.into(),
        timestamp: unix_now()?,
        nonce: random_hex(16),
        payload: serde_json::to_string(proof)?,
        signature: String::new(),
    };
    request.signature = STANDARD.encode(
        key.sign(&request.signing_bytes_for(OQTO_PERSONAL_V1))
            .to_bytes(),
    );
    Ok(request)
}

/// Dial the pinned origin: TLS 1.3 to the exact leaf pin, no proxy,
/// redirect, CA fallback or retry.
async fn dial(pairing: &Pairing, path: &str, max_message: usize) -> Result<Socket> {
    let url = url_parts(&pairing.origin)?;
    let tcp = tokio::time::timeout(
        Duration::from_secs(5),
        TcpStream::connect((url.0.as_str(), url.1)),
    )
    .await
    .context("connect timeout")?
    .context("machine offline")?;
    let _ = tcp.set_nodelay(true);
    let config = oqto_personal_wire::tls::pinned_client_config(&pairing.certificate_sha256)?;
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let name = tokio_rustls::rustls::pki_types::ServerName::try_from(url.0.clone())
        .context("invalid host name")?;
    let tls = tokio::time::timeout(Duration::from_secs(5), connector.connect(name, tcp))
        .await
        .context("TLS timeout")?
        .context("TLS refused (wrong pin?)")?;
    let target = format!("wss://{}:{}{path}", url.2, url.1);
    let ws_config = WebSocketConfig::default()
        .max_message_size(Some(max_message))
        .max_frame_size(Some(max_message));
    let (socket, _) = tokio::time::timeout(
        Duration::from_secs(5),
        tokio_tungstenite::client_async_with_config(target, tls, Some(ws_config)),
    )
    .await
    .context("upgrade timeout")?
    .context("upgrade refused")?;
    Ok(socket)
}

fn url_parts(origin: &str) -> Result<(String, u16, String)> {
    let canonical = oqto_personal_wire::validate_origin(origin)?;
    let rest = canonical
        .strip_prefix("https://")
        .context("https origin")?
        .trim_end_matches('/');
    let (host, port) = match rest.rsplit_once(':') {
        Some((host, port)) if !host.ends_with(']') || host.starts_with('[') => {
            (host.to_string(), port.parse::<u16>().context("port")?)
        }
        _ => (rest.to_string(), 443),
    };
    let bare = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    Ok((bare, port, host))
}

async fn send_json<T: Serialize>(socket: &mut Socket, value: &T) -> Result<()> {
    socket
        .send(Message::Text(wire::encode_json(value)?.into()))
        .await
        .context("send")
}

async fn recv_json<T: for<'de> Deserialize<'de>>(socket: &mut Socket) -> Result<T> {
    loop {
        match socket.next().await.context("closed")?? {
            Message::Text(text) => return wire::decode_json(text.as_str()),
            Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
            Message::Binary(_) => bail!("unexpected binary"),
            Message::Close(_) => bail!("closed"),
        }
    }
}

/// The destination refused the control connection itself.
#[derive(Debug)]
pub struct ControlRefused(pub Refusal);
impl std::fmt::Display for ControlRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "control refused: {}", refusal_name(self.0))
    }
}
impl std::error::Error for ControlRefused {}

pub fn refusal_name(code: Refusal) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "refused".into())
}

/// Outcome of a claim as the guard sees it.
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ClaimResult {
    Claimed {
        call_id: String,
        generation: u64,
        binding: String,
        cwd: String,
        platform: String,
    },
    /// Provably not started: refused before claim, or never sent.
    NotStarted { code: String },
    /// The claim existed already: inspect only.
    Duplicate { receipt: Receipt },
    /// The claim may have been persisted; the reply was lost.
    Unknown,
}

struct ActiveCall {
    attach: Option<String>,
    generation: u64,
    frozen: bool,
    ended: Option<Receipt>,
}

type Waiter = (
    fn(&ServerControl, &str) -> bool,
    oneshot::Sender<ServerControl>,
);

struct Connection {
    ready: Ready,
    tx: mpsc::Sender<ClientControl>,
    closed: tokio_util::sync::CancellationToken,
}

#[derive(Default)]
struct Shared {
    connection: Option<Arc<Connection>>,
    waiters: HashMap<String, Waiter>,
    calls: HashMap<String, ActiveCall>,
    /// Remote stop unconfirmed: no new calls to this machine.
    attention: Option<String>,
}

pub type Events = mpsc::UnboundedSender<serde_json::Value>;

/// One configured machine as seen from the guard.
pub struct MachineLink {
    pub name: String,
    pub config: MachineConfig,
    pub pairing: Pairing,
    key: SigningKey,
    state: LinkState,
    namespace: String,
    controller: Mutex<(u64, u64, bool)>,
    shared: Mutex<Shared>,
    connect: tokio::sync::Mutex<()>,
    events: Events,
}

impl MachineLink {
    pub fn new(
        name: &str,
        config: &MachineConfig,
        state: &LinkState,
        namespace: &str,
        events: Events,
    ) -> Result<Arc<Self>> {
        ensure!(is_hex(namespace, 32), "namespace must be 32 hex characters");
        Ok(Arc::new(Self {
            name: name.into(),
            config: config.clone(),
            pairing: state.pairing(&config.pairing)?,
            key: state.key(&config.pairing)?,
            state: state.clone(),
            namespace: namespace.into(),
            controller: Mutex::new((0, 0, false)),
            shared: Mutex::new(Shared::default()),
            connect: tokio::sync::Mutex::new(()),
            events,
        }))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        match self.shared.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub fn status(&self) -> serde_json::Value {
        let shared = self.lock();
        let binding = shared.connection.as_ref().and_then(|c| {
            c.ready
                .bindings
                .iter()
                .find(|b| b.binding_id == self.config.binding)
                .cloned()
        });
        serde_json::json!({
            "machine": self.name,
            "binding": self.config.binding,
            "lease_ms": self.config.limits.lease_ms,
            "renew_ms": self.config.limits.renew_ms,
            "max_concurrent_calls": self.config.limits.max_concurrent_calls,
            "connected": shared.connection.as_ref().is_some_and(|c| !c.closed.is_cancelled()),
            "host_id": self.pairing.host_id,
            "cwd": binding.as_ref().map(|b| b.cwd.clone()),
            "platform": binding.as_ref().map(|b| b.platform.clone()),
            "remote_attention": binding.as_ref().map(|b| b.attention),
            "attention": shared.attention,
            "active_calls": shared.calls.len(),
        })
    }

    /// Connect (or reuse) the authenticated control connection.
    async fn connection(self: &Arc<Self>) -> Result<Arc<Connection>> {
        let _guard = self.connect.lock().await;
        if let Some(connection) = self.lock().connection.clone()
            && !connection.closed.is_cancelled()
        {
            return Ok(connection);
        }
        let (predecessor, epoch) = {
            let mut controller = match self.controller.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if !controller.2 {
                let (previous, next) = self.state.next_controller(&self.name, &self.namespace)?;
                *controller = (previous, next, true);
            }
            (controller.0, controller.1)
        };
        let mut socket = dial(&self.pairing, CONTROL_PATH, MAX_CONTROL_MESSAGE).await?;
        let ServerControl::Challenge {
            protocol,
            challenge,
        } = tokio::time::timeout(Duration::from_secs(5), recv_json(&mut socket)).await??
        else {
            bail!("expected challenge");
        };
        ensure!(
            protocol == PROTOCOL,
            "unsupported hands protocol {protocol}"
        );
        let proof = sign(
            &self.key,
            &self.pairing.device_id,
            &HandsProof::OpenControl {
                host_id: self.pairing.host_id.clone(),
                challenge,
                client_nonce: random_hex(16),
                namespace: self.namespace.clone(),
                controller_epoch: epoch,
                predecessor_epoch: predecessor,
            },
        )?;
        send_json(&mut socket, &ClientControl::Open { proof }).await?;
        let ready =
            match tokio::time::timeout(Duration::from_secs(10), recv_json(&mut socket)).await?? {
                ServerControl::Ready(ready) => ready,
                ServerControl::Refused { code, .. } => return Err(ControlRefused(code).into()),
                other => bail!("unexpected {other:?}"),
            };
        ensure!(
            ready.protocol == PROTOCOL
                && ready.host_id == self.pairing.host_id
                && ready.controller_epoch == epoch
                && ready.capabilities.iter().any(|c| c == "native")
                && ready.capabilities.iter().any(|c| c == "pi-env.v1"),
            "destination identity or capabilities do not match"
        );
        ensure!(
            ready
                .bindings
                .iter()
                .any(|b| b.binding_id == self.config.binding),
            "binding {} is not offered by {}",
            self.config.binding,
            self.name
        );
        let (tx, rx) = mpsc::channel(64);
        let connection = Arc::new(Connection {
            ready,
            tx,
            closed: tokio_util::sync::CancellationToken::new(),
        });
        {
            let mut shared = self.lock();
            // A new control epoch invalidates every call of the old one.
            let stale: Vec<String> = shared.calls.keys().cloned().collect();
            for id in stale {
                shared.calls.remove(&id);
                let _ = self
                    .events
                    .send(serde_json::json!({"event":"lost","machine":self.name,"call_id":id}));
            }
            shared.connection = Some(Arc::clone(&connection));
        }
        tokio::spawn(Arc::clone(self).run(socket, rx, Arc::clone(&connection)));
        Ok(connection)
    }

    async fn run(
        self: Arc<Self>,
        mut socket: Socket,
        mut rx: mpsc::Receiver<ClientControl>,
        connection: Arc<Connection>,
    ) {
        let mut ping = tokio::time::interval(Duration::from_millis(HEARTBEAT_MS));
        let mut last_seen = tokio::time::Instant::now();
        loop {
            tokio::select! {
                _ = connection.closed.cancelled() => break,
                _ = ping.tick() => {
                    if last_seen.elapsed() > Duration::from_millis(HEALTH_TIMEOUT_MS) { break; }
                    if send_json(&mut socket, &ClientControl::Ping {}).await.is_err() { break; }
                }
                outgoing = rx.recv() => {
                    let Some(message) = outgoing else { break };
                    if send_json(&mut socket, &message).await.is_err() { break; }
                }
                incoming = recv_json::<ServerControl>(&mut socket) => {
                    let Ok(message) = incoming else { break };
                    last_seen = tokio::time::Instant::now();
                    self.dispatch(&connection, message);
                }
            }
        }
        connection.closed.cancel();
        let _ = socket.close(None).await;
        let mut shared = self.lock();
        if !shared
            .connection
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &connection))
        {
            return;
        }
        // Disconnect: every unfinished call is unknown and never replayed.
        let lost: Vec<String> = shared.calls.keys().cloned().collect();
        for id in lost {
            shared.calls.remove(&id);
            let _ = self
                .events
                .send(serde_json::json!({"event":"lost","machine":self.name,"call_id":id}));
        }
        shared.waiters.clear();
    }

    fn dispatch(&self, connection: &Connection, message: ServerControl) {
        let key = match &message {
            ServerControl::Claimed { call_id, .. }
            | ServerControl::Renewed { call_id, .. }
            | ServerControl::RenewChallenge { call_id, .. } => Some(call_id.clone()),
            ServerControl::Refused {
                call_id: Some(call_id),
                ..
            } => Some(call_id.clone()),
            ServerControl::Existing { receipt }
            | ServerControl::Ended { receipt }
            | ServerControl::Inspected { receipt }
            | ServerControl::Released { receipt } => Some(receipt.call_id.clone()),
            ServerControl::FenceAck { binding_id, .. } => Some(format!("fence:{binding_id}")),
            ServerControl::Pong {} => None,
            _ => None,
        };
        if let ServerControl::RenewChallenge {
            call_id,
            sequence,
            challenge,
            ..
        } = &message
        {
            // Only a current, unfrozen call of this guard is renewed.
            let renew = self
                .lock()
                .calls
                .get(call_id)
                .is_some_and(|call| !call.frozen && call.ended.is_none());
            if renew {
                let _ = connection.tx.try_send(ClientControl::Renew {
                    call_id: call_id.clone(),
                    sequence: *sequence,
                    challenge: challenge.clone(),
                });
            }
            return;
        }
        if matches!(
            message,
            ServerControl::Renewed { .. } | ServerControl::Pong {}
        ) {
            return;
        }
        let Some(key) = key else { return };
        let mut shared = self.lock();
        let accepted = shared
            .waiters
            .get(&key)
            .is_some_and(|(accepts, _)| accepts(&message, &key));
        if accepted && let Some((_, tx)) = shared.waiters.remove(&key) {
            let _ = tx.send(message);
            return;
        }
        if let ServerControl::Ended { receipt } = message {
            if let Some(call) = shared.calls.get_mut(&receipt.call_id) {
                call.ended = Some(receipt.clone());
            }
            let _ = self
                .events
                .send(serde_json::json!({"event":"ended","machine":self.name,"receipt":receipt}));
        }
    }

    async fn request(
        self: &Arc<Self>,
        connection: &Connection,
        key: String,
        accepts: fn(&ServerControl, &str) -> bool,
        message: ClientControl,
        timeout: Duration,
    ) -> Result<Option<ServerControl>> {
        let (tx, rx) = oneshot::channel();
        {
            let mut shared = self.lock();
            ensure!(
                !shared.waiters.contains_key(&key),
                "a request for this call is already outstanding"
            );
            shared.waiters.insert(key.clone(), (accepts, tx));
        }
        if connection.tx.try_send(message).is_err() {
            self.lock().waiters.remove(&key);
            connection.closed.cancel();
            bail!("control queue full");
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(reply)) => Ok(Some(reply)),
            _ => {
                self.lock().waiters.remove(&key);
                Ok(None)
            }
        }
    }

    pub async fn claim(
        self: &Arc<Self>,
        call_id: &str,
        digest: &str,
        operation: Operation,
    ) -> ClaimResult {
        if let Some(reason) = self.lock().attention.clone() {
            return ClaimResult::NotStarted {
                code: format!("attention: {reason}"),
            };
        }
        let active = self.lock().calls.len();
        if active >= usize::from(self.config.limits.max_concurrent_calls) {
            return ClaimResult::NotStarted {
                code: "concurrency_exceeded".into(),
            };
        }
        let connection = match self.connection().await {
            Ok(connection) => connection,
            Err(error) => {
                return ClaimResult::NotStarted {
                    code: match error.downcast_ref::<ControlRefused>() {
                        Some(refused) => refusal_name(refused.0),
                        None => format!("offline: {error:#}"),
                    },
                };
            }
        };
        let Some(binding) = connection
            .ready
            .bindings
            .iter()
            .find(|b| b.binding_id == self.config.binding)
            .cloned()
        else {
            return ClaimResult::NotStarted {
                code: "unbound".into(),
            };
        };
        let policy_revision = match self.state.policy_revision(&self.name) {
            Ok(revision) => revision,
            Err(error) => {
                return ClaimResult::NotStarted {
                    code: format!("policy state: {error:#}"),
                };
            }
        };
        let claim = ClaimRequest {
            call_id: call_id.into(),
            binding_id: binding.binding_id.clone(),
            binding_revision: binding.binding_revision.clone(),
            policy_revision,
            digest: digest.into(),
            operation,
            lease_ms: self.config.limits.lease_ms,
            renew_ms: self.config.limits.renew_ms,
            max_concurrent_calls: self.config.limits.max_concurrent_calls,
        };
        // Register before sending so a fast renewal challenge finds the call.
        self.lock().calls.insert(
            call_id.into(),
            ActiveCall {
                attach: None,
                generation: 0,
                frozen: false,
                ended: None,
            },
        );
        let reply = self
            .request(
                &connection,
                call_id.into(),
                |m, _| {
                    matches!(
                        m,
                        ServerControl::Claimed { .. }
                            | ServerControl::Existing { .. }
                            | ServerControl::Refused { .. }
                    )
                },
                ClientControl::Claim(claim),
                Duration::from_secs(10),
            )
            .await;
        match reply {
            Ok(Some(ServerControl::Claimed {
                generation,
                attach_challenge,
                ..
            })) => {
                if let Some(call) = self.lock().calls.get_mut(call_id) {
                    call.attach = Some(attach_challenge);
                    call.generation = generation;
                }
                ClaimResult::Claimed {
                    call_id: call_id.into(),
                    generation,
                    binding: binding.binding_id,
                    cwd: binding.cwd,
                    platform: binding.platform,
                }
            }
            Ok(Some(ServerControl::Existing { receipt })) => {
                self.lock().calls.remove(call_id);
                ClaimResult::Duplicate { receipt }
            }
            Ok(Some(ServerControl::Refused { code, .. })) => {
                self.lock().calls.remove(call_id);
                ClaimResult::NotStarted {
                    code: refusal_name(code),
                }
            }
            _ => {
                self.lock().calls.remove(call_id);
                ClaimResult::Unknown
            }
        }
    }

    /// Connect the control link without any effect, so an offline, refused
    /// or unbound machine fails before the guard admits a call.
    pub async fn probe(self: &Arc<Self>) -> Result<serde_json::Value> {
        if let Some(reason) = self.lock().attention.clone() {
            bail!("attention: {reason}");
        }
        let connection = self.connection().await?;
        let binding = connection
            .ready
            .bindings
            .iter()
            .find(|b| b.binding_id == self.config.binding)
            .context("unbound")?;
        ensure!(!binding.attention, "attention: destination binding frozen");
        Ok(self.status())
    }

    /// Open the call's single data stream. The destination attach challenge
    /// is one-use; a second attach for the same claim is refused.
    pub async fn attach(self: &Arc<Self>, call_id: &str, sync_token: &str) -> Result<DataStream> {
        ensure!(is_hex(sync_token, 32), "invalid sync token");
        let (challenge, generation) = {
            let mut shared = self.lock();
            let call = shared.calls.get_mut(call_id).context("no live claim")?;
            ensure!(
                !call.frozen && call.ended.is_none(),
                "call no longer current"
            );
            (
                call.attach.take().context("attach already used")?,
                call.generation,
            )
        };
        let connection = self
            .lock()
            .connection
            .clone()
            .context("control connection lost")?;
        ensure!(!connection.closed.is_cancelled(), "control connection lost");
        let mut socket = dial(&self.pairing, DATA_PATH, MAX_DATA_MESSAGE).await?;
        let proof = sign(
            &self.key,
            &self.pairing.device_id,
            &HandsProof::AttachData {
                host_id: self.pairing.host_id.clone(),
                challenge,
                control_epoch: connection.ready.control_epoch.clone(),
                call_id: call_id.into(),
                generation,
                sync_token: sync_token.into(),
            },
        )?;
        send_json(&mut socket, &DataHello::Attach { proof }).await?;
        let DataHello::Attached { stream_epoch } =
            tokio::time::timeout(Duration::from_secs(10), recv_json(&mut socket)).await??
        else {
            bail!("expected attached");
        };
        Ok(DataStream {
            socket,
            id: StreamId::new(call_id, generation, &stream_epoch)?,
        })
    }

    pub async fn release(self: &Arc<Self>, call_id: &str) -> Result<Receipt> {
        self.finish(call_id, false).await
    }
    pub async fn cancel(self: &Arc<Self>, call_id: &str) -> Result<Receipt> {
        self.finish(call_id, true).await
    }

    async fn finish(self: &Arc<Self>, call_id: &str, cancel: bool) -> Result<Receipt> {
        let ended = {
            let mut shared = self.lock();
            let ended = shared.calls.get_mut(call_id).and_then(|call| {
                call.frozen = true;
                call.ended.clone()
            });
            if ended.is_some() {
                shared.calls.remove(call_id);
            }
            ended
        };
        if let Some(receipt) = ended {
            return Ok(receipt);
        }
        let connection = self
            .lock()
            .connection
            .clone()
            .filter(|c| !c.closed.is_cancelled())
            .context("control connection lost: outcome unknown")?;
        let message = if cancel {
            ClientControl::Cancel {
                call_id: call_id.into(),
            }
        } else {
            ClientControl::Release {
                call_id: call_id.into(),
            }
        };
        let reply = self
            .request(
                &connection,
                call_id.into(),
                |m, _| {
                    matches!(
                        m,
                        ServerControl::Released { .. }
                            | ServerControl::Ended { .. }
                            | ServerControl::Refused { .. }
                    )
                },
                message,
                Duration::from_secs(25),
            )
            .await?;
        self.lock().calls.remove(call_id);
        match reply {
            Some(ServerControl::Released { receipt } | ServerControl::Ended { receipt }) => {
                if receipt.stop == StopState::Unconfirmed {
                    self.lock().attention = Some("remote stop unconfirmed".into());
                }
                Ok(receipt)
            }
            Some(ServerControl::Refused { code, .. }) => bail!("release refused: {code:?}"),
            _ => bail!("release reply lost: outcome unknown"),
        }
    }

    pub async fn inspect(self: &Arc<Self>, call_id: &str) -> Result<Option<Receipt>> {
        let connection = self.connection().await?;
        let reply = self
            .request(
                &connection,
                call_id.into(),
                |m, _| {
                    matches!(
                        m,
                        ServerControl::Inspected { .. } | ServerControl::Refused { .. }
                    )
                },
                ClientControl::Inspect {
                    call_id: call_id.into(),
                },
                Duration::from_secs(10),
            )
            .await?;
        Ok(match reply {
            Some(ServerControl::Inspected { receipt }) => Some(receipt),
            _ => None,
        })
    }

    /// Origin revoke: freeze locally FIRST (no renewals, no new claims), then
    /// persist the next policy revision and ask the destination to fence,
    /// stop, confirm and acknowledge. Unreachable means remote stop
    /// unconfirmed: the machine stays in attention; leases still expire.
    pub async fn freeze(self: &Arc<Self>) -> serde_json::Value {
        {
            let mut shared = self.lock();
            for call in shared.calls.values_mut() {
                call.frozen = true;
            }
            shared.attention = Some("revoke in progress".into());
        }
        let revision = match self.state.bump_policy(&self.name) {
            Ok(revision) => revision,
            Err(error) => {
                return serde_json::json!({"machine": self.name, "confirmed": false, "error": format!("{error:#}")});
            }
        };
        let result = async {
            let connection = self.connection().await?;
            let binding = self.config.binding.clone();
            self.request(
                &connection,
                format!("fence:{binding}"),
                |m, _| matches!(m, ServerControl::FenceAck { .. }),
                ClientControl::Freeze {
                    binding_id: binding,
                    policy_revision: revision,
                },
                Duration::from_secs(30),
            )
            .await
        }
        .await;
        match result {
            Ok(Some(ServerControl::FenceAck {
                stopped,
                confirmed,
                policy_revision,
                ..
            })) if policy_revision >= revision => {
                let mut shared = self.lock();
                shared.calls.clear();
                shared.attention = if confirmed {
                    None
                } else {
                    Some("remote stop unconfirmed".into())
                };
                serde_json::json!({"machine": self.name, "confirmed": confirmed, "policy_revision": policy_revision, "stopped": stopped})
            }
            _ => {
                self.lock().attention = Some("remote stop unconfirmed".into());
                serde_json::json!({"machine": self.name, "confirmed": false, "policy_revision": revision, "remote_stop": "unconfirmed"})
            }
        }
    }

    /// Stop vouching for a call without a round trip (its authorization ended
    /// or the guard lost it): no further renewals, the fixed lease runs out.
    pub fn withdraw(&self, call_id: &str) {
        if let Some(call) = self.lock().calls.get_mut(call_id) {
            call.frozen = true;
        }
    }

    /// Owner acknowledgement after inspecting an unconfirmed remote stop.
    pub fn clear_attention(&self) {
        self.lock().attention = None;
    }

    /// Test and diagnostics hook: drop the control connection abruptly.
    pub fn disconnect(&self) {
        if let Some(connection) = self.lock().connection.clone() {
            connection.closed.cancel();
        }
    }
}

/// One call's data stream. Every frame is validated against the call,
/// generation, stream epoch and sequence; credit bounds what is in flight.
pub struct DataStream {
    socket: Socket,
    id: StreamId,
}

impl DataStream {
    /// Pump a local duplex (the guard's pi-env pipe) through the stream until
    /// either side ends. Returns the bytes moved in each direction.
    pub async fn pump<L>(mut self, local: L) -> Result<(u64, u64)>
    where
        L: AsyncRead + AsyncWrite + Unpin,
    {
        let (mut read_local, mut write_local) = tokio::io::split(local);
        let mut reader = FrameReader::new(self.id);
        let mut credit = SendCredit::default();
        let mut window = ReceiveWindow::default();
        let mut seq = 0u64;
        let mut buffer = vec![0u8; MAX_DATA_PAYLOAD];
        let (mut sent, mut received) = (0u64, 0u64);
        let mut local_done = false;
        loop {
            let limit = credit.available().min(buffer.len());
            let readable = !local_done && limit > 0;
            tokio::select! {
                read = read_local.read(&mut buffer[..limit.max(1)]), if readable => {
                    let n = read?;
                    if n == 0 {
                        local_done = true;
                        let frame = encode_frame(&self.id, DataKind::End, seq, &[])?;
                        seq += 1;
                        self.socket.send(Message::Binary(frame.into())).await?;
                        continue;
                    }
                    credit.spend(n)?;
                    let frame = encode_frame(&self.id, DataKind::Env, seq, &buffer[..n])?;
                    seq += 1;
                    self.socket.send(Message::Binary(frame.into())).await?;
                    sent += n as u64;
                }
                message = self.socket.next() => {
                    let Some(message) = message else { break };
                    let bytes = match message? {
                        Message::Binary(bytes) => bytes,
                        Message::Close(_) => break,
                        Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
                        Message::Text(_) => bail!("text on a data stream"),
                    };
                    let (kind, payload) = reader.read(&bytes)?;
                    match kind {
                        DataKind::Env => {
                            window.receive(payload.len())?;
                            write_local.write_all(payload).await?;
                            write_local.flush().await?;
                            received += payload.len() as u64;
                            let granted = window.release(payload.len());
                            let frame = encode_frame(&self.id, DataKind::Credit, seq, &granted.to_be_bytes())?;
                            seq += 1;
                            self.socket.send(Message::Binary(frame.into())).await?;
                        }
                        DataKind::Credit => credit.grant(credit_value(payload)?)?,
                        DataKind::End => {
                            let _ = write_local.shutdown().await;
                        }
                    }
                }
            }
        }
        let _ = self.socket.close(None).await;
        let _ = write_local.shutdown().await;
        Ok((sent, received))
    }
}

/// Every configured machine for one guard namespace.
pub struct Links {
    pub machines: HashMap<String, Arc<MachineLink>>,
    pub refused: HashMap<String, String>,
}

impl Links {
    pub fn open(config: &OriginConfig, state: &LinkState, namespace: &str, events: Events) -> Self {
        let mut machines = HashMap::new();
        let mut refused = HashMap::new();
        for (name, machine) in &config.machines {
            match MachineLink::new(name, machine, state, namespace, events.clone()) {
                Ok(link) => {
                    machines.insert(name.clone(), link);
                }
                Err(error) => {
                    refused.insert(name.clone(), format!("{error:#}"));
                }
            }
        }
        Self { machines, refused }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_url_parts() -> Result<()> {
        assert_eq!(
            url_parts("https://127.0.0.1:7443/")?,
            ("127.0.0.1".into(), 7443, "127.0.0.1".into())
        );
        assert_eq!(
            url_parts("https://host.example/")?,
            ("host.example".into(), 443, "host.example".into())
        );
        assert!(url_parts("http://host/").is_err());
        Ok(())
    }
}
