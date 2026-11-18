//! Destination enforcer and standalone foreground hands host (ADR-0086).
//!
//! The host never asks anyone to approve a call. It verifies the paired
//! device, the owner's standing delegation, the controller, the fences and the
//! lease, persists the claim, and runs one confined generation per call. A
//! revoke, fence, takeover, disconnect or expiry stops and confirms the
//! generation; a stop that cannot be confirmed freezes the binding with
//! attention until the owner clears it.
use crate::{
    clock::Clock,
    config::{BindingConfig, HostConfig, LinkLimits},
    envframe::RequestFilter,
    journal::{Activation, ClaimOutcome, Journal, NewClaim},
    launcher::{Generation, LaunchBinding, Launcher},
    lease::{Lease, RenewError},
    wire::{self, *},
};
use anyhow::{Context, Result, bail, ensure};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use oqto_personal::{Access, DeviceFence, PairingAuthority};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, watch},
};

pub struct Binding {
    pub id: String,
    pub root: PathBuf,
    pub revision: String,
    pub config: BindingConfig,
}

impl Binding {
    fn info(&self, attention: bool) -> BindingInfo {
        BindingInfo {
            binding_id: self.id.clone(),
            binding_revision: self.revision.clone(),
            cwd: self.root.display().to_string(),
            platform: std::env::consts::OS.into(),
            max_lease_ms: self.config.max_lease_ms,
            max_concurrent_calls: self.config.max_concurrent_calls,
            workspace_mode: "native".into(),
            attention,
        }
    }
}

struct Control {
    device_id: String,
    namespace: String,
    fence: DeviceFence,
    tx: mpsc::Sender<ServerControl>,
    closed: tokio_util::sync::CancellationToken,
}

/// Shared per-call state; the data task and the enforcer meet here.
struct CallShared {
    call_id: String,
    device_id: String,
    namespace: String,
    control_epoch: String,
    binding_id: String,
    generation: u64,
    deadline_ms: AtomicU64,
    bytes_in: AtomicU64,
    bytes_out: AtomicU64,
    attached: AtomicBool,
    /// Requested end reason; the data task stops its generation on change.
    stop: watch::Sender<Option<CallState>>,
    /// The data task's confirmed stop result.
    stopped: watch::Sender<Option<StopState>>,
    /// The final receipt once recorded.
    done: watch::Sender<Option<Receipt>>,
}

struct LiveCall {
    shared: Arc<CallShared>,
    lease: Lease,
    renew_ms: u64,
    last_offer_ms: u64,
    attach: Option<(String, u64)>,
    fence: DeviceFence,
}

#[derive(Default)]
struct State {
    controls: HashMap<String, Control>,
    calls: HashMap<String, LiveCall>,
}

pub struct HandsHost {
    pub authority: Arc<PairingAuthority>,
    pub journal: Journal,
    launcher: Arc<dyn Launcher>,
    clock: Arc<dyn Clock>,
    pub host_boot: String,
    pub bindings: BTreeMap<String, Binding>,
    state: Mutex<State>,
}

fn digest_binding(id: &str, root: &std::path::Path, config: &BindingConfig) -> String {
    let mut hash = Sha256::new();
    hash.update(id.as_bytes());
    hash.update([0]);
    hash.update(root.as_os_str().as_encoded_bytes());
    hash.update([0]);
    hash.update(config.max_lease_ms.to_be_bytes());
    hash.update([config.max_concurrent_calls]);
    hex::encode(&hash.finalize()[..16])
}

impl HandsHost {
    pub fn new(
        config: &HostConfig,
        authority: Arc<PairingAuthority>,
        journal: Journal,
        launcher: Arc<dyn Launcher>,
        clock: Arc<dyn Clock>,
    ) -> Result<Arc<Self>> {
        let mut bindings = BTreeMap::new();
        for (id, binding) in &config.bindings {
            let root = binding
                .root
                .canonicalize()
                .with_context(|| format!("binding {id}: resolving root"))?;
            ensure!(root.is_dir(), "binding {id}: root must be a directory");
            bindings.insert(
                id.clone(),
                Binding {
                    id: id.clone(),
                    revision: digest_binding(id, &root, binding),
                    root,
                    config: binding.clone(),
                },
            );
        }
        Ok(Arc::new(Self {
            authority,
            journal,
            launcher,
            clock,
            host_boot: random_hex(16),
            bindings,
            state: Mutex::new(State::default()),
        }))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        // A poisoned lock means a panic mid-update: refuse rather than guess.
        match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn now(&self) -> u64 {
        self.clock.now_ms()
    }

    /// Background enforcement: lease expiry, renewal challenges, device fences.
    pub fn spawn_enforcer(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let host = Arc::clone(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(100));
            loop {
                tick.tick().await;
                host.enforce_once().await;
            }
        })
    }

    async fn enforce_once(self: &Arc<Self>) {
        let now = self.now();
        let mut expired = Vec::new();
        let mut fenced = Vec::new();
        {
            let mut state = self.lock();
            let State { controls, calls } = &mut *state;
            for (id, call) in calls.iter_mut() {
                if !call.fence.is_current() {
                    fenced.push(id.clone());
                    continue;
                }
                if !call.lease.alive(now) {
                    expired.push(id.clone());
                    continue;
                }
                if now.saturating_sub(call.last_offer_ms) >= call.renew_ms
                    && let Some(control) = controls.get(&call.shared.control_epoch)
                {
                    let challenge = random_hex(16);
                    if let Ok(offer) = call.lease.offer(now, challenge) {
                        call.last_offer_ms = now;
                        let message = ServerControl::RenewChallenge {
                            call_id: id.clone(),
                            sequence: offer.sequence,
                            challenge: offer.challenge,
                            remaining_ms: offer.deadline_ms.saturating_sub(now),
                        };
                        if control.tx.try_send(message).is_err() {
                            control.closed.cancel();
                        }
                    }
                }
            }
            // A revoked device's control connections close too.
            for control in controls.values() {
                if !control.fence.is_current() {
                    control.closed.cancel();
                }
            }
        }
        for id in expired {
            let host = Arc::clone(self);
            tokio::spawn(async move {
                let _ = host.end_call(&id, CallState::Expired).await;
            });
        }
        for id in fenced {
            let host = Arc::clone(self);
            tokio::spawn(async move {
                let _ = host.end_call(&id, CallState::Fenced).await;
            });
        }
    }

    /// Stop one call for `reason`, confirm its generation, record the receipt.
    /// Idempotent: concurrent callers wait for the same receipt.
    pub async fn end_call(self: &Arc<Self>, call_id: &str, reason: CallState) -> Result<Receipt> {
        // Decide the end reason and read `attached` under the state lock, the
        // same lock the data path attaches under, so a call is either stopped
        // before attach or its generation is stopped and confirmed.
        let found = {
            let state = self.lock();
            state.calls.get(call_id).map(|call| {
                let shared = Arc::clone(&call.shared);
                let first = shared.stop.send_if_modified(|current| {
                    if current.is_none() {
                        *current = Some(reason);
                        true
                    } else {
                        false
                    }
                });
                let attached = shared.attached.load(Ordering::SeqCst);
                (shared, first, attached)
            })
        };
        let Some((shared, first, attached)) = found else {
            return self
                .journal
                .receipt(call_id)
                .await?
                .context("no claim for this call");
        };
        let mut done = shared.done.subscribe();
        if !first {
            let receipt = done
                .wait_for(Option::is_some)
                .await
                .map_err(|_| anyhow::anyhow!("call ended without a receipt"))?
                .clone();
            return receipt.context("call ended without a receipt");
        }
        let stop = if attached {
            let mut stopped = shared.stopped.subscribe();
            match tokio::time::timeout(Duration::from_secs(15), stopped.wait_for(Option::is_some))
                .await
            {
                Ok(Ok(value)) => value.unwrap_or(StopState::Unconfirmed),
                _ => StopState::Unconfirmed,
            }
        } else {
            StopState::NotStarted
        };
        let bytes = (
            shared.bytes_in.load(Ordering::SeqCst),
            shared.bytes_out.load(Ordering::SeqCst),
        );
        let receipt = self
            .journal
            .transition(call_id, reason, stop, Some(bytes))
            .await?;
        if stop == StopState::Unconfirmed {
            self.journal
                .set_attention(&shared.binding_id, "hands stop not confirmed")
                .await?;
            log::warn!(
                "binding {} frozen: generation {} stop not confirmed",
                shared.binding_id,
                shared.generation
            );
        }
        let control = {
            let mut state = self.lock();
            state.calls.remove(call_id);
            state
                .controls
                .get(&shared.control_epoch)
                .map(|c| c.tx.clone())
        };
        if let Some(tx) = control
            && reason != CallState::Released
        {
            let _ = tx.try_send(ServerControl::Ended {
                receipt: receipt.clone(),
            });
        }
        shared.done.send_replace(Some(receipt.clone()));
        Ok(receipt)
    }

    /// Stop every call matching `filter` (fence, revoke, takeover, disconnect).
    async fn end_matching(
        self: &Arc<Self>,
        reason: CallState,
        filter: impl Fn(&CallShared) -> bool,
    ) -> Vec<StoppedGeneration> {
        let ids: Vec<String> = {
            let state = self.lock();
            state
                .calls
                .values()
                .filter(|call| filter(&call.shared))
                .map(|call| call.shared.call_id.clone())
                .collect()
        };
        let mut stopped = Vec::new();
        for id in ids {
            match self.end_call(&id, reason).await {
                Ok(receipt) => stopped.push(StoppedGeneration {
                    call_id: receipt.call_id,
                    generation: receipt.generation,
                    stop: receipt.stop,
                }),
                Err(error) => {
                    log::warn!("ending call failed: {error:#}");
                    stopped.push(StoppedGeneration {
                        call_id: id,
                        generation: 0,
                        stop: StopState::Unconfirmed,
                    });
                }
            }
        }
        stopped
    }

    /// Owner revoke of a paired device: personal revocation first (no new
    /// admissions), then stop and confirm every hands generation it holds.
    pub async fn revoke_device(self: &Arc<Self>, device_id: &str) -> Result<serde_json::Value> {
        let revoked = self.authority.revoke(device_id).await?;
        let stopped = self
            .end_matching(CallState::Fenced, |call| call.device_id == device_id)
            .await;
        {
            let state = self.lock();
            for control in state.controls.values() {
                if control.device_id == device_id {
                    control.closed.cancel();
                }
            }
        }
        let confirmed = stopped.iter().all(|s| s.stop != StopState::Unconfirmed);
        Ok(serde_json::json!({"revoked": revoked, "stopped": stopped, "confirmed": confirmed}))
    }

    /// Host shutdown: every live call becomes unknown; generations stop.
    pub async fn shutdown(self: &Arc<Self>) -> usize {
        self.end_matching(CallState::Unknown, |_| true).await.len()
    }

    pub async fn undelegate(
        self: &Arc<Self>,
        device_id: &str,
        binding_id: &str,
    ) -> Result<serde_json::Value> {
        let removed = self.journal.undelegate(device_id, binding_id).await?;
        let stopped = self
            .end_matching(CallState::Fenced, |call| {
                call.device_id == device_id && call.binding_id == binding_id
            })
            .await;
        let confirmed = stopped.iter().all(|s| s.stop != StopState::Unconfirmed);
        Ok(serde_json::json!({"removed": removed, "stopped": stopped, "confirmed": confirmed}))
    }

    pub async fn status(&self) -> Result<serde_json::Value> {
        let attention = self.journal.attention().await?;
        let bindings: Vec<_> = self
            .bindings
            .values()
            .map(|b| {
                let info = b.info(attention.iter().any(|(id, _)| id == &b.id));
                serde_json::json!({
                    "binding_id": info.binding_id,
                    "cwd": info.cwd,
                    "revision": info.binding_revision,
                    "max_lease_ms": info.max_lease_ms,
                    "max_concurrent_calls": info.max_concurrent_calls,
                    "attention": info.attention,
                })
            })
            .collect();
        let (controls, calls) = {
            let state = self.lock();
            let now = self.now();
            let controls: Vec<_> = state
                .controls
                .iter()
                .map(|(epoch, c)| serde_json::json!({"control_epoch": &epoch[..8], "device_id": c.device_id, "namespace": c.namespace}))
                .collect();
            let calls: Vec<_> = state
                .calls
                .values()
                .map(|call| {
                    serde_json::json!({
                        "call_id": call.shared.call_id,
                        "binding_id": call.shared.binding_id,
                        "generation": call.shared.generation,
                        "attached": call.shared.attached.load(Ordering::SeqCst),
                        "lease_remaining_ms": call.lease.remaining_ms(now),
                    })
                })
                .collect();
            (controls, calls)
        };
        Ok(serde_json::json!({
            "host_id": self.authority.host_id(),
            "host_boot": self.host_boot,
            "bindings": bindings,
            "attention": attention.iter().map(|(b, r)| serde_json::json!({"binding_id": b, "reason": r})).collect::<Vec<_>>(),
            "delegations": self.journal.delegations().await?.iter().map(|(d, b)| serde_json::json!({"device_id": d, "binding_id": b})).collect::<Vec<_>>(),
            "controls": controls,
            "calls": calls,
        }))
    }

    // ---------------------------------------------------------------- control

    pub async fn serve_control(self: Arc<Self>, socket: WebSocket) {
        if let Err(error) = self.control_session(socket).await {
            log::info!("control session ended: {error:#}");
        }
    }

    async fn authenticate(
        &self,
        proof: &oqto_personal_wire::SignedRequest,
    ) -> Result<(HandsProof, DeviceFence)> {
        let device = self
            .authority
            .authenticate(proof)
            .await
            .context("device proof refused")?;
        let parsed = HandsProof::parse(&proof.payload)?;
        // Inventory admission checks the current device revision; the hands
        // delegation is checked separately and never implied by pairing.
        let action = self
            .authority
            .require_current(&device, Access::Inventory)
            .await
            .context("device not current")?;
        let fence = action.fence();
        drop(action);
        Ok((parsed, fence))
    }

    async fn control_session(self: &Arc<Self>, socket: WebSocket) -> Result<()> {
        let (mut sink, mut stream) = socket.split();
        let challenge = random_hex(16);
        send_text(
            &mut sink,
            &ServerControl::Challenge {
                protocol: PROTOCOL.into(),
                challenge: challenge.clone(),
            },
        )
        .await?;
        let open: ClientControl = tokio::time::timeout(
            Duration::from_millis(CHALLENGE_TIMEOUT_MS),
            next_text(&mut stream),
        )
        .await
        .context("control open timed out")??;
        let ClientControl::Open { proof } = open else {
            bail!("expected open");
        };
        let (parsed, fence) = self.authenticate(&proof).await?;
        let HandsProof::OpenControl {
            host_id,
            challenge: answered,
            client_nonce,
            namespace,
            controller_epoch,
            predecessor_epoch,
        } = parsed
        else {
            bail!("expected an open_control proof");
        };
        ensure!(
            host_id == self.authority.host_id() && answered == challenge,
            "proof bound to another host or challenge"
        );
        ensure!(
            is_hex(&client_nonce, 32) && is_hex(&namespace, 32),
            "invalid nonce or namespace"
        );
        let device_id = fence.device_id().to_string();
        let delegated = self
            .journal
            .delegations()
            .await?
            .iter()
            .any(|(d, _)| d == &device_id);
        if !delegated {
            send_text(
                &mut sink,
                &ServerControl::Refused {
                    call_id: None,
                    code: Refusal::NotDelegated,
                },
            )
            .await?;
            bail!("device has no hands delegation");
        }
        let live = {
            let state = self.lock();
            state
                .controls
                .values()
                .any(|c| c.device_id == device_id && c.namespace == namespace)
        };
        match self
            .journal
            .activate_controller(
                &device_id,
                &namespace,
                predecessor_epoch,
                controller_epoch,
                live,
            )
            .await?
        {
            Activation::Refused => {
                send_text(
                    &mut sink,
                    &ServerControl::Refused {
                        call_id: None,
                        code: Refusal::StaleController,
                    },
                )
                .await?;
                bail!("controller epoch refused");
            }
            Activation::Activated {
                previous: Some(previous),
            } => {
                // Takeover: fence the old controller and stop its generations
                // before the new epoch can claim anything.
                let device = device_id.clone();
                let ns = namespace.clone();
                let stopped = self
                    .end_matching(CallState::Fenced, |call| {
                        call.device_id == device && call.namespace == ns
                    })
                    .await;
                let state = self.lock();
                for control in state.controls.values() {
                    if control.device_id == device_id && control.namespace == namespace {
                        control.closed.cancel();
                    }
                }
                log::info!(
                    "controller takeover {previous}->{controller_epoch}: {} generations stopped",
                    stopped.len()
                );
            }
            Activation::Activated { previous: None } | Activation::Resumed => {}
        }
        let control_epoch = random_hex(16);
        let attention = self.journal.attention().await?;
        let ready = Ready {
            protocol: PROTOCOL.into(),
            host_id: self.authority.host_id().into(),
            host_boot: self.host_boot.clone(),
            device_revision: fence.generation(),
            control_epoch: control_epoch.clone(),
            controller_epoch,
            bindings: self
                .bindings
                .values()
                .map(|b| b.info(attention.iter().any(|(id, _)| id == &b.id)))
                .collect(),
            capabilities: vec!["native".into(), "pi-env.v1".into()],
        };
        let (tx, mut rx) = mpsc::channel::<ServerControl>(64);
        let closed = tokio_util::sync::CancellationToken::new();
        self.lock().controls.insert(
            control_epoch.clone(),
            Control {
                device_id: device_id.clone(),
                namespace: namespace.clone(),
                fence: fence.clone(),
                tx: tx.clone(),
                closed: closed.clone(),
            },
        );
        send_text(&mut sink, &ServerControl::Ready(ready)).await?;
        let writer_closed = closed.clone();
        let writer = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = writer_closed.cancelled() => break,
                    message = rx.recv() => {
                        let Some(message) = message else { break };
                        if send_text(&mut sink, &message).await.is_err() { break; }
                    }
                }
            }
            let _ = sink.close().await;
        });
        let session = ControlSession {
            device_id: device_id.clone(),
            namespace,
            controller_epoch,
            control_epoch: control_epoch.clone(),
            fence,
            tx,
        };
        let result = loop {
            let message = tokio::select! {
                _ = closed.cancelled() => break Ok(()),
                message = tokio::time::timeout(Duration::from_millis(HEALTH_TIMEOUT_MS), next_text::<ClientControl>(&mut stream)) => message,
            };
            let message = match message {
                Ok(Ok(message)) => message,
                Ok(Err(error)) => break Err(error),
                Err(_) => break Err(anyhow::anyhow!("control health timeout")),
            };
            if let Err(error) = self.handle_control(&session, message).await {
                break Err(error);
            }
        };
        closed.cancel();
        let _ = writer.await;
        self.lock().controls.remove(&control_epoch);
        // Disconnect: unfinished calls of this control are unknown, never
        // replayed; their generations stop now rather than at lease expiry.
        let epoch = control_epoch.clone();
        self.end_matching(CallState::Unknown, |call| call.control_epoch == epoch)
            .await;
        result
    }

    async fn handle_control(
        self: &Arc<Self>,
        session: &ControlSession,
        message: ClientControl,
    ) -> Result<()> {
        let reply = match message {
            ClientControl::Open { .. } => bail!("duplicate open"),
            ClientControl::Ping {} => ServerControl::Pong {},
            ClientControl::Claim(claim) => self.claim(session, claim).await?,
            ClientControl::Renew {
                call_id,
                sequence,
                challenge,
            } => {
                let now = self.now();
                let answer = {
                    let mut state = self.lock();
                    match state.calls.get_mut(&call_id) {
                        Some(call) if call.shared.control_epoch == session.control_epoch => {
                            let result = call.lease.answer(now, sequence, &challenge);
                            if let Ok(deadline) = result {
                                call.shared.deadline_ms.store(deadline, Ordering::SeqCst);
                            }
                            Some(result)
                        }
                        _ => None,
                    }
                };
                match answer {
                    Some(Ok(deadline)) => ServerControl::Renewed {
                        call_id,
                        sequence,
                        remaining_ms: deadline.saturating_sub(now),
                    },
                    Some(Err(RenewError::Expired)) => {
                        let receipt = self.end_call(&call_id, CallState::Expired).await?;
                        ServerControl::Ended { receipt }
                    }
                    Some(Err(RenewError::NotOutstanding)) | None => ServerControl::Refused {
                        call_id: Some(call_id),
                        code: Refusal::StalePolicy,
                    },
                }
            }
            ClientControl::Cancel { call_id } => match self.owned(session, &call_id).await? {
                true => ServerControl::Ended {
                    receipt: self.end_call(&call_id, CallState::Cancelled).await?,
                },
                false => refused(Some(call_id), Refusal::NotFound),
            },
            ClientControl::Release { call_id } => match self.owned(session, &call_id).await? {
                true => ServerControl::Released {
                    receipt: self.end_call(&call_id, CallState::Released).await?,
                },
                false => refused(Some(call_id), Refusal::NotFound),
            },
            ClientControl::Inspect { call_id } => match self.owned(session, &call_id).await? {
                true => match self.journal.receipt(&call_id).await? {
                    Some(receipt) => ServerControl::Inspected { receipt },
                    None => refused(Some(call_id), Refusal::NotFound),
                },
                false => refused(Some(call_id), Refusal::NotFound),
            },
            ClientControl::Freeze {
                binding_id,
                policy_revision,
            } => {
                ensure!(
                    self.bindings.contains_key(&binding_id),
                    "freeze for an unknown binding"
                );
                // Persist the fence before stopping anything or accepting more.
                self.journal
                    .raise_fence(&session.device_id, &binding_id, policy_revision)
                    .await?;
                let device = session.device_id.clone();
                let binding = binding_id.clone();
                let stopped = self
                    .end_matching(CallState::Fenced, |call| {
                        call.device_id == device && call.binding_id == binding
                    })
                    .await;
                let confirmed = stopped.iter().all(|s| s.stop != StopState::Unconfirmed);
                let current = self.journal.fence(&session.device_id, &binding_id).await?;
                ServerControl::FenceAck {
                    binding_id,
                    policy_revision: current,
                    stopped,
                    confirmed,
                }
            }
        };
        session
            .tx
            .try_send(reply)
            .map_err(|_| anyhow::anyhow!("control queue overflow"))
    }

    async fn owned(&self, session: &ControlSession, call_id: &str) -> Result<bool> {
        Ok(self
            .journal
            .claim_owner(call_id)
            .await?
            .is_some_and(|(device, ns)| device == session.device_id && ns == session.namespace))
    }

    async fn claim(
        self: &Arc<Self>,
        session: &ControlSession,
        claim: ClaimRequest,
    ) -> Result<ServerControl> {
        let call = Some(claim.call_id.clone());
        if !is_hex(&claim.call_id, 32) || !is_hex(&claim.digest, 64) {
            return Ok(refused(call, Refusal::InvalidRequest));
        }
        let Some(binding) = self.bindings.get(&claim.binding_id) else {
            return Ok(refused(call, Refusal::UnknownBinding));
        };
        if claim.binding_revision != binding.revision {
            return Ok(refused(call, Refusal::StaleBinding));
        }
        let limits = LinkLimits {
            lease_ms: claim.lease_ms,
            renew_ms: claim.renew_ms,
            max_concurrent_calls: claim.max_concurrent_calls,
        };
        if limits.validate("claim").is_err() || claim.lease_ms > binding.config.max_lease_ms {
            return Ok(refused(call, Refusal::LimitExceeded));
        }
        if !session.fence.is_current() {
            return Ok(refused(call, Refusal::DeviceRevoked));
        }
        let active = {
            let state = self.lock();
            state
                .calls
                .values()
                .filter(|c| {
                    c.shared.device_id == session.device_id
                        && c.shared.binding_id == claim.binding_id
                })
                .count()
        };
        let cap = usize::from(
            claim
                .max_concurrent_calls
                .min(binding.config.max_concurrent_calls),
        );
        let existing = self.journal.receipt(&claim.call_id).await?;
        if existing.is_none() && active >= cap {
            return Ok(refused(call, Refusal::ConcurrencyExceeded));
        }
        let cwd = binding.root.display().to_string();
        let outcome = self
            .journal
            .claim(&NewClaim {
                call_id: &claim.call_id,
                device_id: &session.device_id,
                namespace: &session.namespace,
                controller_epoch: session.controller_epoch,
                binding_id: &claim.binding_id,
                policy_revision: claim.policy_revision,
                digest: &claim.digest,
                operation: claim.operation,
                cwd: &cwd,
                host_boot: &self.host_boot,
            })
            .await?;
        let generation = match outcome {
            ClaimOutcome::Existing(receipt) => return Ok(ServerControl::Existing { receipt }),
            ClaimOutcome::Refused(code) => return Ok(refused(call, code)),
            ClaimOutcome::Fresh { generation } => generation,
        };
        let now = self.now();
        let lease = Lease::start(now, claim.lease_ms);
        let attach = random_hex(16);
        let shared = Arc::new(CallShared {
            call_id: claim.call_id.clone(),
            device_id: session.device_id.clone(),
            namespace: session.namespace.clone(),
            control_epoch: session.control_epoch.clone(),
            binding_id: claim.binding_id.clone(),
            generation,
            deadline_ms: AtomicU64::new(lease.deadline_ms()),
            bytes_in: AtomicU64::new(0),
            bytes_out: AtomicU64::new(0),
            attached: AtomicBool::new(false),
            stop: watch::channel(None).0,
            stopped: watch::channel(None).0,
            done: watch::channel(None).0,
        });
        let remaining = lease.remaining_ms(now);
        self.lock().calls.insert(
            claim.call_id.clone(),
            LiveCall {
                shared,
                lease,
                renew_ms: u64::from(claim.renew_ms),
                last_offer_ms: now,
                attach: Some((attach.clone(), now + ATTACH_TTL_MS)),
                fence: session.fence.clone(),
            },
        );
        Ok(ServerControl::Claimed {
            call_id: claim.call_id,
            generation,
            remaining_ms: remaining,
            attach_challenge: attach,
        })
    }

    // ------------------------------------------------------------------- data

    pub async fn serve_data(self: Arc<Self>, socket: WebSocket) {
        if let Err(error) = self.data_session(socket).await {
            log::info!("data stream ended: {error:#}");
        }
    }

    async fn data_session(self: &Arc<Self>, socket: WebSocket) -> Result<()> {
        let (mut sink, mut stream) = socket.split();
        let hello: DataHello = tokio::time::timeout(
            Duration::from_millis(CHALLENGE_TIMEOUT_MS),
            next_text(&mut stream),
        )
        .await
        .context("data attach timed out")??;
        let DataHello::Attach { proof } = hello else {
            bail!("expected attach");
        };
        let (parsed, fence) = self.authenticate(&proof).await?;
        let HandsProof::AttachData {
            host_id,
            challenge,
            control_epoch,
            call_id,
            generation,
            sync_token,
        } = parsed
        else {
            bail!("expected an attach_data proof");
        };
        ensure!(
            host_id == self.authority.host_id(),
            "proof for another host"
        );
        let now = self.now();
        // One-use attach: take the challenge whatever the outcome.
        let (shared, binding_root) = {
            let mut state = self.lock();
            let control_live = state.controls.contains_key(&control_epoch);
            let call = state.calls.get_mut(&call_id).context("no live claim")?;
            let attach = call.attach.take();
            ensure!(
                call.shared.device_id == fence.device_id()
                    && call.shared.control_epoch == control_epoch
                    && control_live
                    && call.shared.generation == generation,
                "attach for a stale control, generation or device"
            );
            ensure!(
                attach.is_some_and(|(expected, expires)| expected == challenge && now < expires),
                "attach challenge invalid, consumed or expired"
            );
            ensure!(call.lease.alive(now), "lease expired before attach");
            ensure!(call.shared.stop.borrow().is_none(), "call already ending");
            let root = self
                .bindings
                .get(&call.shared.binding_id)
                .map(|b| b.root.clone())
                .context("binding gone")?;
            call.shared.attached.store(true, Ordering::SeqCst);
            (Arc::clone(&call.shared), root)
        };
        // Durable before any byte can reach a daemon.
        let running = self
            .journal
            .transition(&call_id, CallState::Running, StopState::Pending, None)
            .await;
        if running.is_err() || shared.stop.borrow().is_some() {
            // Ended meanwhile (or the journal failed): nothing was launched.
            shared.stopped.send_replace(Some(StopState::NotStarted));
            running?;
            bail!("call ended before launch");
        }
        let mut generation_handle = match self
            .launcher
            .launch(
                &LaunchBinding { root: binding_root },
                generation,
                &sync_token,
            )
            .await
        {
            Ok(handle) => handle,
            Err(error) => {
                // The launcher failed before returning a child it could own.
                shared.stopped.send_replace(Some(StopState::NotStarted));
                let host = Arc::clone(self);
                let id = call_id.clone();
                tokio::spawn(async move {
                    let _ = host.end_call(&id, CallState::Unknown).await;
                });
                return Err(error.context("launching hands generation"));
            }
        };
        let stream_epoch = random_hex(16);
        let result = async {
            send_text(
                &mut sink,
                &DataHello::Attached {
                    stream_epoch: stream_epoch.clone(),
                },
            )
            .await?;
            let id = StreamId::new(&call_id, generation, &stream_epoch)?;
            self.pump(
                &shared,
                &fence,
                generation_handle.as_mut(),
                id,
                &mut sink,
                &mut stream,
            )
            .await
        }
        .await;
        let stop = generation_handle.stop().await;
        shared.stopped.send_replace(Some(stop));
        let _ = sink.close().await;
        // Expiry or fence noticed on the data path ends the call; a closed
        // stream waits for the origin's release, inspect or lease expiry.
        if let Err(reason) = &result
            && let Some(state) = reason.downcast_ref::<EndReason>()
        {
            let host = Arc::clone(self);
            let state = state.0;
            tokio::spawn(async move {
                let _ = host.end_call(&call_id, state).await;
            });
        }
        result.map(|_| ())
    }

    async fn pump(
        &self,
        shared: &CallShared,
        fence: &DeviceFence,
        generation: &mut dyn Generation,
        id: StreamId,
        sink: &mut futures::stream::SplitSink<WebSocket, Message>,
        stream: &mut futures::stream::SplitStream<WebSocket>,
    ) -> Result<()> {
        let (mut stdin, mut stdout) = generation.take_io().context("generation io")?;
        let mut reader = FrameReader::new(id);
        let mut filter = RequestFilter::default();
        let mut window = ReceiveWindow::default();
        let mut credit = SendCredit::default();
        let mut seq = 0u64;
        let mut stop = shared.stop.subscribe();
        let mut buffer = vec![0u8; MAX_DATA_PAYLOAD];
        let mut out_done = false;
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        let alive = |host: &Self| {
            host.now() < shared.deadline_ms.load(Ordering::SeqCst) && fence.is_current()
        };
        loop {
            if let Some(reason) = *stop.borrow_and_update() {
                bail!(EndReason(reason));
            }
            let readable = !out_done && credit.available() > 0;
            let limit = credit.available().min(buffer.len());
            tokio::select! {
                _ = stop.changed() => {
                    bail!(EndReason(stop.borrow().unwrap_or(CallState::Unknown)));
                }
                _ = tick.tick() => {
                    if !alive(self) {
                        bail!(EndReason(if fence.is_current() { CallState::Expired } else { CallState::Fenced }));
                    }
                }
                message = stream.next() => {
                    let Some(message) = message else { return Ok(()) };
                    let bytes = match message.context("data socket error")? {
                        Message::Binary(bytes) => bytes,
                        Message::Close(_) => return Ok(()),
                        Message::Ping(_) | Message::Pong(_) => continue,
                        Message::Text(_) => bail!("text on a data stream"),
                    };
                    // A suspended or partitioned host must refuse stale frames
                    // before any byte reaches the daemon.
                    if !alive(self) {
                        bail!(EndReason(if fence.is_current() { CallState::Expired } else { CallState::Fenced }));
                    }
                    let (kind, payload) = reader.read(&bytes)?;
                    match kind {
                        DataKind::Env => {
                            window.receive(payload.len())?;
                            let mut forward = Vec::with_capacity(payload.len());
                            filter.push(payload, &mut forward)?;
                            stdin.write_all(&forward).await.context("daemon input closed")?;
                            stdin.flush().await?;
                            shared.bytes_in.fetch_add(payload.len() as u64, Ordering::SeqCst);
                            let granted = window.release(payload.len());
                            sink.send(Message::Binary(encode_frame(&id, DataKind::Credit, seq, &granted.to_be_bytes())?.into())).await?;
                            seq += 1;
                        }
                        DataKind::Credit => credit.grant(credit_value(payload)?)?,
                        DataKind::End => stdin.shutdown().await?,
                    }
                }
                read = stdout.read(&mut buffer[..limit.max(1)]), if readable => {
                    let n = read.context("daemon output")?;
                    if n == 0 {
                        out_done = true;
                        sink.send(Message::Binary(encode_frame(&id, DataKind::End, seq, &[])?.into())).await?;
                        seq += 1;
                        continue;
                    }
                    credit.spend(n)?;
                    shared.bytes_out.fetch_add(n as u64, Ordering::SeqCst);
                    sink.send(Message::Binary(encode_frame(&id, DataKind::Env, seq, &buffer[..n])?.into())).await?;
                    seq += 1;
                }
            }
        }
    }
}

struct ControlSession {
    device_id: String,
    namespace: String,
    controller_epoch: u64,
    control_epoch: String,
    fence: DeviceFence,
    tx: mpsc::Sender<ServerControl>,
}

#[derive(Debug)]
struct EndReason(CallState);
impl std::fmt::Display for EndReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "call ended: {}", self.0.as_str())
    }
}
impl std::error::Error for EndReason {}

fn refused(call_id: Option<String>, code: Refusal) -> ServerControl {
    ServerControl::Refused { call_id, code }
}

async fn send_text<T: serde::Serialize>(
    sink: &mut futures::stream::SplitSink<WebSocket, Message>,
    value: &T,
) -> Result<()> {
    sink.send(Message::Text(wire::encode_json(value)?.into()))
        .await
        .context("control send")
}

async fn next_text<T: for<'de> serde::Deserialize<'de>>(
    stream: &mut futures::stream::SplitStream<WebSocket>,
) -> Result<T> {
    loop {
        match stream.next().await.context("connection closed")?? {
            Message::Text(text) => return wire::decode_json(text.as_str()),
            Message::Ping(_) | Message::Pong(_) => continue,
            Message::Binary(_) => bail!("binary before attach"),
            Message::Close(_) => bail!("connection closed"),
        }
    }
}
