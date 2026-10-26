//! Controlled input is distinct from observation. Only authenticated host
//! authority may issue a permit; neither a wire boolean nor an extension lease
//! is MFA/Account evidence. The native connection must additionally implement
//! terminal-confirmed, connection-scoped leases (legacy hubs are observe-only).
use super::*;
use serde::Serialize;
use std::{
    net::Shutdown,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

const MAX_MFA_AGE: Duration = Duration::from_secs(300);
const MAX_LEASE: Duration = Duration::from_secs(60);

/// Different issuers remain different evidence. Pairing is not Account MFA.
#[derive(Clone)]
enum Assurance {
    VerifiedAccountMfa { verified_at: Instant },
    PairedDevice(oqto_personal::DeviceFence),
}
/// Opaque in-process resource/control permit, never a wire boolean or tool
/// approval. Both issuers still require native terminal-confirmed input leases.
#[derive(Clone)]
pub struct ControlPermit {
    authority_id: String,
    public_session_id: String,
    controller_id: String,
    generation: String,
    assurance: Assurance,
    expires_at: Instant,
}
impl ControlPermit {
    pub fn from_authenticated_host(
        account_id: String,
        public_session_id: String,
        controller_id: String,
        generation: String,
        mfa_verified_at: Instant,
        expires_at: Instant,
    ) -> Result<Self> {
        let permit = Self {
            authority_id: account_id,
            public_session_id,
            controller_id,
            generation,
            assurance: Assurance::VerifiedAccountMfa {
                verified_at: mfa_verified_at,
            },
            expires_at,
        };
        permit.check_time()?;
        for id in [
            &permit.authority_id,
            &permit.public_session_id,
            &permit.controller_id,
            &permit.generation,
        ] {
            ensure!(
                !id.trim().is_empty() && id.len() <= 256,
                "Invalid control identity"
            );
        }
        Ok(permit)
    }
    /// Mint only from the authority's sealed CURRENT control admission. The
    /// returned fence can outlive bounded dispatch but closes on host revocation.
    pub fn from_paired_device(
        admission: &oqto_personal::AuthorizedAction,
        public_session_id: String,
    ) -> Result<Self> {
        ensure!(
            admission.grants().sessions == oqto_personal::SessionAccess::Control,
            "Paired Session control not granted"
        );
        ensure!(
            !public_session_id.is_empty() && public_session_id.len() <= 256,
            "Invalid public Session"
        );
        let fence = admission.fence();
        let permit = Self {
            authority_id: format!("personal-host:{}", fence.host_id()),
            public_session_id,
            controller_id: fence.device_id().into(),
            generation: fence.generation(),
            assurance: Assurance::PairedDevice(fence),
            expires_at: Instant::now() + MAX_LEASE,
        };
        permit.check_time()?;
        Ok(permit)
    }
    pub fn controller_id(&self) -> &str {
        &self.controller_id
    }
    pub fn generation(&self) -> &str {
        &self.generation
    }
    fn check_time(&self) -> Result<()> {
        let now = Instant::now();
        match &self.assurance {
            Assurance::VerifiedAccountMfa { verified_at } => ensure!(
                now.checked_duration_since(*verified_at)
                    .is_some_and(|age| age <= MAX_MFA_AGE),
                "Fresh verified MFA required"
            ),
            Assurance::PairedDevice(fence) => ensure!(fence.is_current(), "Paired Device revoked"),
        }
        ensure!(
            self.expires_at
                .checked_duration_since(now)
                .is_some_and(|remaining| remaining <= MAX_LEASE && !remaining.is_zero()),
            "Control permit expired or overlong"
        );
        Ok(())
    }
    fn check_binding(&self, binding: &ObservationBinding, controller: &str) -> Result<()> {
        self.check_time()?;
        ensure!(
            self.authority_id == binding.authority_id
                && self.public_session_id == binding.public_session_id
                && self.controller_id == controller,
            "Control identity denied"
        );
        Ok(())
    }
}

pub trait ControlAuthority: Send + Sync {
    /// Resolve controller identity from authenticated host state, never from
    /// caller-supplied Account, MFA flags, timestamps or certificate inventory.
    fn authorize_control(
        &self,
        binding: &ObservationBinding,
        controller: &str,
    ) -> Result<ControlPermit>;
    /// Recheck current membership, grant generation, logout and revocation.
    fn revalidate_control(
        &self,
        binding: &ObservationBinding,
        permit: &ControlPermit,
    ) -> Result<()>;
}

/// Intentionally no arbitrary command, shell, path, session-switch or guard
/// approval operation. Durable/oqt adapters must use their own guarded facade.
#[derive(Deserialize, Serialize)]
#[serde(
    tag = "command",
    content = "args",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum InputAction {
    Prompt { message: String },
    Steer { message: String },
    FollowUp { message: String },
    SetModel { provider: String, model_id: String },
    SetThinkingLevel { level: String },
    Abort,
}
impl InputAction {
    fn packet(&self) -> Result<Value> {
        let (kind, message) = match self {
            Self::Prompt { message } => ("prompt", Some(message)),
            Self::Steer { message } => ("steer", Some(message)),
            Self::FollowUp { message } => ("follow_up", Some(message)),
            Self::SetModel { provider, model_id } => {
                for part in [provider, model_id] {
                    ensure!(
                        !part.trim().is_empty()
                            && part.len() <= 512
                            && !part.chars().any(char::is_control),
                        "Invalid model identity"
                    );
                }
                return Ok(
                    json!({"type":"set_model","id":uuid::Uuid::new_v4().to_string(),"provider":provider,"modelId":model_id}),
                );
            }
            Self::SetThinkingLevel { level } => {
                ensure!(
                    !level.trim().is_empty()
                        && level.len() <= 32
                        && level
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
                    "Invalid thinking level"
                );
                return Ok(
                    json!({"type":"set_thinking_level","id":uuid::Uuid::new_v4().to_string(),"level":level}),
                );
            }
            Self::Abort => ("abort", None),
        };
        let mut packet = json!({"type":kind,"id":uuid::Uuid::new_v4().to_string()});
        if let Some(message) = message {
            ensure!(
                !message.trim().is_empty() && message.len() <= 32 * 1024,
                "Invalid input size"
            );
            packet["message"] = message.clone().into();
        }
        Ok(packet)
    }
}
#[derive(Debug, Serialize)]
pub struct InputReceipt {
    pub public_session_id: String,
    /// Transport correlation only; not an oqto-log message/replay ID.
    pub request_id: String,
}

/// The watchdog can shut down the shared socket even while the controller is
/// idle. It owns no harness PID, and cannot approve/revoke any tool permission.
pub(super) struct InputLease {
    permit: Arc<ControlPermit>,
    authority: Arc<dyn ControlAuthority>,
    active: Arc<AtomicBool>,
    socket: std::os::unix::net::UnixStream,
    watchdog: tokio::task::JoinHandle<()>,
}
impl Drop for InputLease {
    fn drop(&mut self) {
        self.active.store(false, Ordering::SeqCst);
        let _ = self.socket.shutdown(Shutdown::Both);
        self.watchdog.abort();
    }
}
impl InputLease {
    fn start(
        reader: &BufReader<UnixStream>,
        binding: ObservationBinding,
        observe: Arc<dyn ObservationAuthority>,
        authority: Arc<dyn ControlAuthority>,
        permit: Arc<ControlPermit>,
    ) -> Result<Self> {
        use std::os::fd::AsFd;
        let socket =
            std::os::unix::net::UnixStream::from(reader.get_ref().as_fd().try_clone_to_owned()?);
        let watcher = socket.try_clone()?;
        let active = Arc::new(AtomicBool::new(true));
        let (flag, auth, grant) = (active.clone(), authority.clone(), permit.clone());
        let watchdog = tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(25));
            loop {
                tick.tick().await;
                if !flag.load(Ordering::SeqCst)
                    || grant.check_time().is_err()
                    || observe.revalidate(&binding).is_err()
                    || auth.revalidate_control(&binding, &grant).is_err()
                {
                    flag.store(false, Ordering::SeqCst);
                    let _ = watcher.shutdown(Shutdown::Both);
                    break;
                }
            }
        });
        Ok(Self {
            permit,
            authority,
            active,
            socket,
            watchdog,
        })
    }
    pub(super) fn active(&self) -> Arc<AtomicBool> {
        self.active.clone()
    }
    fn check(&self, binding: &ObservationBinding) -> Result<()> {
        ensure!(self.active.load(Ordering::SeqCst), "Input lease ended");
        self.permit
            .check_binding(binding, &self.permit.controller_id)?;
        self.authority.revalidate_control(binding, &self.permit)
    }
}

impl TuiObserver {
    pub fn control_supported(&self) -> bool {
        self.connection_leases
    }

    /// Waits for native terminal confirmation, never asks for a password or
    /// borrows another socket's lease. Cancellation/denial closes this socket.
    pub async fn acquire_input(
        &mut self,
        authority: Arc<dyn ControlAuthority>,
        controller: &str,
    ) -> Result<()> {
        self.authority.revalidate(&self.binding)?;
        ensure!(
            self.connection_leases,
            "Native transport lacks connection-scoped input leases"
        );
        ensure!(self.input_lease.is_none(), "Input already acquired");
        let permit = Arc::new(authority.authorize_control(&self.binding, controller)?);
        permit.check_binding(&self.binding, controller)?;
        authority.revalidate_control(&self.binding, &permit)?;
        let mut reader = self.reader.take().context("TUI attachment disconnected")?;
        let packet =
            json!({"id":uuid::Uuid::new_v4().to_string(),"type":"lease","action":"request"});
        let fence = ExchangeAuthority {
            binding: &self.binding,
            observe: self.authority.clone(),
            control: authority.clone(),
            permit: permit.clone(),
        };
        let reply = exchange(
            &mut reader,
            &mut self.pending,
            fence,
            packet,
            Duration::from_secs(30),
            false,
            self.snapshot_only,
        )
        .await?;
        ensure!(
            reply["data"]["owner"] == "remote" && reply["data"]["reason"] == "tui_confirmed",
            "Fresh terminal confirmation required"
        );
        authority.revalidate_control(&self.binding, &permit)?;
        permit.check_binding(&self.binding, controller)?;
        self.input_lease = Some(InputLease::start(
            &reader,
            self.binding.clone(),
            self.authority.clone(),
            authority,
            permit,
        )?);
        self.reader = Some(reader);
        Ok(())
    }

    /// No automatic retries: timeout/EOF after write has an uncertain outcome.
    /// Accepted actions may already have effects; lease revoke is not rollback.
    pub async fn input(&mut self, action: InputAction) -> Result<InputReceipt> {
        self.validate_setting(&action).await?;
        let packet = action.packet()?;
        let request_id = packet["id"]
            .as_str()
            .context("Missing request identity")?
            .to_owned();
        self.authority.revalidate(&self.binding)?;
        // Own both resources in the future: cancelled/failed requests close the
        // native socket AND its watchdog, never leave remote control lingering.
        let lease = self
            .input_lease
            .take()
            .context("Input control not acquired")?;
        lease.check(&self.binding)?;
        let mut reader = self.reader.take().context("TUI attachment disconnected")?;
        let fence = ExchangeAuthority {
            binding: &self.binding,
            observe: self.authority.clone(),
            control: lease.authority.clone(),
            permit: lease.permit.clone(),
        };
        exchange(
            &mut reader,
            &mut self.pending,
            fence,
            packet,
            Duration::from_secs(5),
            true,
            self.snapshot_only,
        )
        .await
        .context("Input outcome uncertain; do not automatically retry")?;
        lease.check(&self.binding)?;
        self.reader = Some(reader);
        self.input_lease = Some(lease);
        Ok(InputReceipt {
            public_session_id: self.binding.public_session_id.clone(),
            request_id,
        })
    }

    /// Closing the owned connection reclaims its lease, not another observer's.
    pub fn release_input(&mut self) {
        self.input_lease.take();
        self.reader.take();
        self.pending.clear();
    }
}

struct ExchangeAuthority<'a> {
    binding: &'a ObservationBinding,
    observe: Arc<dyn ObservationAuthority>,
    control: Arc<dyn ControlAuthority>,
    permit: Arc<ControlPermit>,
}
async fn exchange(
    reader: &mut BufReader<UnixStream>,
    pending: &mut VecDeque<ObservedEvent>,
    fence: ExchangeAuthority<'_>,
    packet: Value,
    deadline: Duration,
    held: bool,
    snapshot_only: bool,
) -> Result<Value> {
    let id = packet["id"].clone();
    let command = packet["type"].clone();
    let check = || -> Result<()> {
        fence.observe.revalidate(fence.binding)?;
        fence
            .permit
            .check_binding(fence.binding, &fence.permit.controller_id)?;
        fence
            .control
            .revalidate_control(fence.binding, &fence.permit)
    };
    check()?;
    let operation = async {
        reader
            .get_mut()
            .write_all(&serde_json::to_vec(&packet)?)
            .await?;
        reader.get_mut().write_all(b"\n").await?;
        loop {
            let frame = read_frame(reader).await?;
            match frame["type"].as_str() {
                Some("lease") => {
                    ensure!(!held || frame["owner"] != "tui", "Terminal reclaimed input")
                }
                Some("event") => {
                    ensure!(
                        pending.len() < MAX_PENDING,
                        "TUI event gap; canonical resnapshot required"
                    );
                    let native = event(frame)?;
                    if !snapshot_only {
                        pending.push_back(native);
                    }
                }
                Some("response") => {
                    ensure!(
                        frame["id"] == id && frame["command"] == command,
                        "Unexpected input response"
                    );
                    ensure!(frame["success"] == true, "Native input rejected");
                    check()?;
                    return Ok(frame);
                }
                _ => bail!("Unexpected input frame"),
            }
        }
    };
    tokio::pin!(operation);
    let timeout = tokio::time::sleep(deadline);
    tokio::pin!(timeout);
    let mut revocation = tokio::time::interval(Duration::from_millis(25));
    loop {
        tokio::select! {
            result = &mut operation => return result,
            _ = revocation.tick() => check()?,
            _ = &mut timeout => bail!("Input deadline exceeded"),
        }
    }
}

#[cfg(test)]
mod personal_options_test;
#[cfg(test)]
mod tests;
