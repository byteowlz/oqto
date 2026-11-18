//! Synthetic loopback peer, ephemeral port and in-memory platform stores only.
//! No Runner/server crate, live Host, user key, transcript, fixed port or FFI.
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use oqto_personal_client::*;
use oqto_personal_wire::{
    Enrollment, OQTO_PERSONAL_V1, PairingQr, SignedRequest, actions::HostAction,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Notify,
};
use tokio_rustls::{
    TlsAcceptor,
    rustls::{
        ServerConfig,
        pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer},
    },
};

pub const SESSION: &str = "oqto-synthetic-session";
pub const HOST: &str = "00000000-0000-4000-8000-000000000001";
pub const DEVICE: &str = "00000000-0000-4000-8000-000000000002";
pub const ATTACHMENT: &str = "00000000-0000-4000-8000-000000000003";

#[derive(Default)]
pub struct Stores {
    pub hosts: Mutex<HashMap<String, Host>>,
    pub secrets: Mutex<HashMap<String, String>>,
    pub fail_secrets: std::sync::atomic::AtomicBool,
}
impl HostStore for Stores {
    fn load(&self, id: &str) -> Result<Option<Host>> {
        Ok(self.hosts.lock().unwrap().get(id).cloned())
    }
    fn reserve(&self, pending: &Host) -> Result<()> {
        let mut rows = self.hosts.lock().unwrap();
        if rows.contains_key(&pending.id) {
            return Err(ClientError::AlreadyRecorded);
        }
        rows.insert(pending.id.clone(), pending.clone());
        Ok(())
    }
    fn replace(&self, expected: &Host, replacement: &Host) -> Result<()> {
        let mut rows = self.hosts.lock().unwrap();
        if rows.get(&expected.id) != Some(expected) {
            return Err(ClientError::Storage);
        }
        rows.insert(expected.id.clone(), replacement.clone());
        Ok(())
    }
}
impl CredentialStore for Stores {
    fn load(&self, key: &str) -> Result<Option<String>> {
        if self.fail_secrets.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(ClientError::Storage);
        }
        Ok(self.secrets.lock().unwrap().get(key).cloned())
    }
    fn save(&self, key: &str, value: &str) -> Result<()> {
        if self.fail_secrets.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(ClientError::Storage);
        }
        self.secrets
            .lock()
            .unwrap()
            .insert(key.into(), value.into());
        Ok(())
    }
    fn remove(&self, key: &str) -> Result<()> {
        self.secrets.lock().unwrap().remove(key);
        Ok(())
    }
}

pub struct State {
    key: Option<VerifyingKey>,
    nonces: HashSet<String>,
    pub counts: HashMap<&'static str, usize>,
    pub denied: bool,
    pub not_ready: bool,
    pub history_unavailable: bool,
    pub duplicate_catalog: bool,
    pub input_disconnect: bool,
    pub bad_receipt: bool,
    pub read: Value,
    pub history: Value,
    pub pause: Option<&'static str>,
    pub response_bytes: Option<Vec<u8>>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            key: None,
            nonces: HashSet::new(),
            counts: HashMap::new(),
            denied: false,
            not_ready: false,
            history_unavailable: false,
            duplicate_catalog: false,
            input_disconnect: false,
            bad_receipt: false,
            pause: None,
            response_bytes: None,
            read: json!({"session_id":SESSION,"streaming":false,"compacting":false,"input_held":true,
                "native_leaf_id":"leaf", "details":{"model":{"provider":"synthetic","id":"fixture","name":"Fixture"},
                "context_usage":{"tokens":0,"context_window":100}}}),
            history: page(),
        }
    }
}

pub fn page() -> Value {
    json!({"messages":[{"id":"message","session_id":SESSION,"role":"assistant","created_at":1,"completed_at":2,
        "model_id":"synthetic-model","provider_id":"synthetic","tokens_input":42,
        "parts":[{"id":"message:part:0","part_type":"error","text":"Synthetic provider refused.",
            "tool_output":{"retained":[1,2]},"future_field":true}]}],"has_more":false,"next_before":null,
        "native_graph":{"source":"pi-native-graph-v1","branch_id":"branch","revision":1,"native_leaf_id":"leaf","live":true}})
}

pub struct Peer {
    pub qr: Vec<u8>,
    pub state: Arc<Mutex<State>>,
    pub entered: Arc<Notify>,
    pub release: Arc<Notify>,
    task: tokio::task::JoinHandle<()>,
}
impl Peer {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("https://{}/", listener.local_addr().unwrap());
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let der = cert.cert.der().clone();
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()));
        let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
        let config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![der.clone()], key)
            .unwrap();
        let state = Arc::new(Mutex::new(State::default()));
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let task_state = state.clone();
        let task_entered = entered.clone();
        let task_release = release.clone();
        let task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let acceptor = acceptor.clone();
                let state = task_state.clone();
                let entered = task_entered.clone();
                let release = task_release.clone();
                tokio::spawn(async move {
                    let Ok(mut tls) = acceptor.accept(stream).await else {
                        return;
                    };
                    let mut bytes = Vec::new();
                    let header_end = loop {
                        if bytes.len() > 2 * 1024 * 1024 {
                            return;
                        }
                        let mut block = [0u8; 4096];
                        let Ok(n) = tls.read(&mut block).await else {
                            return;
                        };
                        if n == 0 {
                            return;
                        }
                        bytes.extend_from_slice(&block[..n]);
                        if let Some(index) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                            break index + 4;
                        }
                    };
                    let header = std::str::from_utf8(&bytes[..header_end]).unwrap();
                    let path = header
                        .lines()
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .nth(1)
                        .unwrap()
                        .to_owned();
                    let length = header
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if length > 1024 * 1024 {
                        return;
                    }
                    while bytes.len() < header_end + length {
                        let mut block = [0u8; 4096];
                        let Ok(n) = tls.read(&mut block).await else {
                            return;
                        };
                        if n == 0 {
                            return;
                        }
                        bytes.extend_from_slice(&block[..n]);
                    }
                    let value: Value =
                        serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
                    let (status, body, paused, disconnect) = respond(&state, &path, value);
                    if paused {
                        entered.notify_one();
                        release.notified().await;
                    }
                    if disconnect {
                        return;
                    }
                    let body = state
                        .lock()
                        .unwrap()
                        .response_bytes
                        .clone()
                        .unwrap_or_else(|| serde_json::to_vec(&body).unwrap());
                    let header = format!(
                        "HTTP/1.1 {status} Synthetic\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = tls.write_all(header.as_bytes()).await;
                    let _ = tls.write_all(&body).await;
                    let _ = tls.shutdown().await;
                });
            }
        });
        let now = now();
        let qr = PairingQr::new(HOST, &origin, der.as_ref(), "a".repeat(64), now + 300, now)
            .unwrap()
            .to_json()
            .unwrap();
        Self {
            qr,
            state,
            entered,
            release,
            task,
        }
    }
    pub fn count(&self, name: &'static str) -> usize {
        *self.state.lock().unwrap().counts.get(name).unwrap_or(&0)
    }
    pub async fn enrolled(&self) -> (Arc<PersonalClient>, Arc<Stores>, Host) {
        let stores = Arc::new(Stores::default());
        let client = Arc::new(PersonalClient::new(stores.clone(), stores.clone()));
        let host = client
            .enroll_once(&self.qr, "Synthetic Host", "Synthetic desktop")
            .await
            .unwrap();
        client.catalog(&host.id).await.unwrap();
        (client, stores, host)
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.task.abort();
        self.release.notify_waiters();
    }
}

fn respond(shared: &Mutex<State>, path: &str, value: Value) -> (u16, Value, bool, bool) {
    let mut state = shared.lock().unwrap();
    if path == oqto_personal_wire::ENROLL_PATH {
        let enrollment: Enrollment = serde_json::from_value(value).unwrap();
        let key_bytes: [u8; 32] = STANDARD
            .decode(&enrollment.public_key)
            .unwrap()
            .try_into()
            .unwrap();
        let key = VerifyingKey::from_bytes(&key_bytes).unwrap();
        let signature =
            Signature::from_slice(&STANDARD.decode(&enrollment.signature).unwrap()).unwrap();
        key.verify(&enrollment.signing_bytes_for(OQTO_PERSONAL_V1), &signature)
            .unwrap();
        assert_eq!(enrollment.token, "a".repeat(64));
        assert!(
            state.key.is_none(),
            "one-use enrollment must not be replayed"
        );
        state.key = Some(key);
        *state.counts.entry("enroll").or_default() += 1;
        return (
            200,
            json!({"version":1,"host_id":HOST,"device_id":DEVICE}),
            state.pause == Some("enroll"),
            false,
        );
    }
    assert_eq!(path, oqto_personal_wire::actions::ACTION_PATH);
    let proof: SignedRequest = serde_json::from_value(value).unwrap();
    assert_eq!(proof.device_id, DEVICE);
    assert!(
        state.nonces.insert(proof.nonce.clone()),
        "each exchange needs a fresh nonce"
    );
    let signature = Signature::from_slice(&STANDARD.decode(&proof.signature).unwrap()).unwrap();
    state
        .key
        .unwrap()
        .verify(&proof.signing_bytes_for(OQTO_PERSONAL_V1), &signature)
        .unwrap();
    let action: HostAction = serde_json::from_str(&proof.payload).unwrap();
    let name = match &action {
        HostAction::Capabilities => "capabilities",
        HostAction::SessionsList => "catalog",
        HostAction::SessionHistory { .. } => "history",
        HostAction::SessionAttach { .. } => "attach",
        HostAction::SessionRead { .. } => "read",
        HostAction::SessionRequestControl { .. } => "control",
        HostAction::SessionInput { .. } => "input",
        HostAction::SessionDetach { .. } => "detach",
        _ => panic!("unexpected action"),
    };
    *state.counts.entry(name).or_default() += 1;
    let paused = state.pause == Some(name);
    if state.denied {
        return (403, json!({"error":"denied"}), paused, false);
    }
    if state.not_ready && matches!(name, "attach" | "read")
        || state.history_unavailable && name == "history"
    {
        return (503, json!({"error":"unavailable"}), paused, false);
    }
    let body = match action {
        HostAction::Capabilities => {
            json!({"version":1,"host_id":HOST,"device_id":DEVICE,"roots":[],
            "session_observe_granted":true,"session_control_granted":true,"full_user_terminal_granted":false,
            "files_supported":false,"terminal_supported":false,"session_observe_supported":true,"session_control_supported":true})
        }
        HostAction::SessionsList => {
            let row =
                json!({"id":SESSION,"workspace":"/synthetic/work","title":"Synthetic","live":true});
            json!({"sessions": if state.duplicate_catalog { vec![row.clone(), row] } else { vec![row] }})
        }
        HostAction::SessionHistory { session_id, .. } => {
            assert_eq!(session_id, SESSION);
            state.history.clone()
        }
        HostAction::SessionAttach { session_id } => {
            assert_eq!(session_id, SESSION);
            json!({"attachment_id":ATTACHMENT,"session_id":SESSION,"streaming":false,"compacting":false,"control_supported":true,"native_leaf_id":"leaf"})
        }
        HostAction::SessionRead { attachment_id } => {
            assert_eq!(attachment_id, ATTACHMENT);
            state.read.clone()
        }
        HostAction::SessionRequestControl { attachment_id } => {
            assert_eq!(attachment_id, ATTACHMENT);
            json!({"input_held":true})
        }
        HostAction::SessionInput { attachment_id, .. } => {
            assert_eq!(attachment_id, ATTACHMENT);
            json!({"public_session_id":if state.bad_receipt {"wrong-session"} else {SESSION},"request_id":"synthetic-request"})
        }
        HostAction::SessionDetach { attachment_id } => {
            assert_eq!(attachment_id, ATTACHMENT);
            json!({"detached":true})
        }
        _ => unreachable!(),
    };
    (200, body, paused, name == "input" && state.input_disconnect)
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
