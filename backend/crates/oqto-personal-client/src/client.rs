use crate::{ClientError, Host, PairingPreview, Result, credential::Credential, transport};
use oqto_personal_wire::{EnrollmentReply, PairingQr, actions::HostAction};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;

/// Platform Keychain/Keystore seam. Values are opaque credential JSON, never
/// host-registry data. Implementations must not log keys or values.
pub trait CredentialStore: Send + Sync {
    fn load(&self, key: &str) -> Result<Option<String>>;
    fn save(&self, key: &str, value: &str) -> Result<()>;
    fn remove(&self, key: &str) -> Result<()>;
}

/// Nonsecret host persistence. Mutation methods MUST be atomic across users of
/// this adapter: reserve refuses an existing ID; replace compares the complete
/// expected record (including registration_id), preserving unrelated records.
/// A genuinely absent first-run registry permits load -> None. Corrupt/newer
/// or unavailable existing storage is an error, never an empty registry.
pub trait HostStore: Send + Sync {
    fn load(&self, host_id: &str) -> Result<Option<Host>>;
    fn reserve(&self, pending: &Host) -> Result<()>;
    fn replace(&self, expected: &Host, replacement: &Host) -> Result<()>;
}

/// Local-only QR preview. No enrollment, key generation, storage or network I/O.
pub fn pairing_preview(payload: &[u8], now: i64) -> Result<PairingPreview> {
    let qr = PairingQr::parse(payload, now).map_err(|_| ClientError::InvalidInput)?;
    Ok(PairingPreview {
        host_id: qr.host_id().into(),
        origin: qr.origin().into(),
        certificate_sha256: qr.certificate_sha256().into(),
        expires_at: qr.expires_at(),
    })
}

pub(crate) struct Data {
    pub generation: u64,
    pub active: bool,
    pub catalogs: HashMap<String, (String, Vec<crate::Session>)>,
    pub attachments: HashMap<(String, String), (String, crate::Attachment)>,
    pub histories: HashMap<(String, String), crate::history::Paging>,
    pub observations: HashMap<(String, String), crate::sessions::Recovery>,
}

/// Clone/share an Arc of this client rather than making per-screen instances.
/// A new process starts with no catalog, attachment, input or cursor authority.
pub struct PersonalClient {
    pub(crate) hosts: Arc<dyn HostStore>,
    pub(crate) credentials: Arc<dyn CredentialStore>,
    data: Mutex<Data>,
    changes: watch::Sender<u64>,
    pub(crate) session_gate: tokio::sync::Mutex<()>,
    pub(crate) observation_gate: tokio::sync::Mutex<()>,
    pub(crate) history_gate: tokio::sync::Mutex<()>,
    transport: transport::Transport,
}

impl PersonalClient {
    pub fn new(hosts: Arc<dyn HostStore>, credentials: Arc<dyn CredentialStore>) -> Self {
        let (changes, _) = watch::channel(0);
        Self {
            hosts,
            credentials,
            changes,
            data: Mutex::new(Data {
                generation: 0,
                active: true,
                catalogs: HashMap::new(),
                attachments: HashMap::new(),
                histories: HashMap::new(),
                observations: HashMap::new(),
            }),
            session_gate: tokio::sync::Mutex::new(()),
            observation_gate: tokio::sync::Mutex::new(()),
            history_gate: tokio::sync::Mutex::new(()),
            transport: transport::Transport::default(),
        }
    }

    pub(crate) fn data(&self) -> Result<MutexGuard<'_, Data>> {
        self.data.lock().map_err(|_| ClientError::Storage)
    }

    /// Cancel in-flight exchanges and discard all local admission. No remote
    /// input, automatic detach, enrollment replay or process termination occurs.
    /// Remote leases retain server-owned expiry/reclaim rules.
    pub fn invalidate(&self) -> Result<()> {
        let mut data = self.data()?;
        self.invalidate_data(&mut data)
    }

    pub fn set_active(&self, active: bool) -> Result<()> {
        let mut data = self.data()?;
        self.invalidate_data(&mut data)?;
        data.active = active;
        Ok(())
    }

    fn invalidate_data(&self, data: &mut Data) -> Result<()> {
        data.active &= data.generation < u64::MAX;
        data.generation = data
            .generation
            .checked_add(1)
            .ok_or(ClientError::Cancelled)?;
        data.catalogs.clear();
        data.attachments.clear();
        data.histories.clear();
        data.observations.clear();
        self.transport.clear();
        self.changes.send_replace(data.generation);
        Ok(())
    }

    pub(crate) fn check(data: &Data, generation: u64) -> Result<()> {
        if data.active && data.generation == generation {
            Ok(())
        } else {
            Err(ClientError::Cancelled)
        }
    }

    pub(crate) fn host(&self, id: &str) -> Result<Host> {
        let host = self.hosts.load(id)?.ok_or(ClientError::Denied)?;
        validate_host(&host)?;
        if !host.paired || host.id != id {
            return Err(ClientError::Denied);
        }
        Ok(host)
    }

    pub(crate) fn known_session(&self, host: &str, session: &str) -> Result<()> {
        let row = self.host(host)?;
        let data = self.data()?;
        if !data.active
            || !data.catalogs.get(host).is_some_and(|(registration, rows)| {
                registration == &row.registration_id && rows.iter().any(|r| r.id == session)
            })
        {
            return Err(ClientError::Denied);
        }
        Ok(())
    }

    pub(crate) async fn request(&self, host_id: &str, action: HostAction) -> Result<(Value, u64)> {
        let mut changed = self.changes.subscribe();
        let generation = self.data()?.generation;
        Self::check(&*self.data()?, generation)?;
        let host = self.host(host_id)?;
        let key = self.credential(&host)?;
        Self::check(&*self.data()?, generation)?;
        let uncertain = transport::mutating(&action);
        let result = tokio::select! {
            biased;
            _ = changed.changed() => Err(if uncertain { ClientError::OutcomeUnknown } else { ClientError::Cancelled }),
            result = self.transport.action(&host, &key, &action) => result,
        };
        let current = self.hosts.load(host_id).map_err(|error| {
            if uncertain {
                ClientError::OutcomeUnknown
            } else {
                error
            }
        })?;
        let mut data = self.data().map_err(|error| {
            if uncertain {
                ClientError::OutcomeUnknown
            } else {
                error
            }
        })?;
        if Self::check(&data, generation).is_err() || current.as_ref() != Some(&host) {
            return Err(if uncertain {
                ClientError::OutcomeUnknown
            } else {
                ClientError::Cancelled
            });
        }
        if matches!(result, Err(ClientError::Denied | ClientError::PinMismatch)) {
            self.invalidate_data(&mut data)?;
        }
        result.map(|value| (value, generation))
    }

    fn credential(&self, host: &Host) -> Result<Credential> {
        let value = self
            .credentials
            .load(&credential_key(host))?
            .ok_or(ClientError::Denied)?;
        let key: Credential = serde_json::from_str(&value).map_err(|_| ClientError::Storage)?;
        key.validate()?;
        Ok(key)
    }

    fn save_credential(&self, host: &Host, key: &Credential) -> Result<()> {
        let value = serde_json::to_string(key).map_err(|_| ClientError::Storage)?;
        let name = credential_key(host);
        self.credentials.save(&name, &value)?;
        if self.credentials.load(&name)?.as_deref() != Some(&value) {
            return Err(ClientError::Storage);
        }
        Ok(())
    }

    /// Reserve pending custody BEFORE sending. Any failure preserves the pending
    /// record and key; another call refuses rather than replaying a one-use ticket.
    pub async fn enroll_once(
        &self,
        payload: &[u8],
        label: &str,
        device_name: &str,
    ) -> Result<Host> {
        let mut changed = self.changes.subscribe();
        let generation = self.data()?.generation;
        Self::check(&*self.data()?, generation)?;
        let qr = PairingQr::parse(payload, now()?).map_err(|_| ClientError::InvalidInput)?;
        short_label(label)?;
        short_label(device_name)?;
        let pending = Host {
            id: qr.host_id().into(),
            label: label.trim().into(),
            origin: qr.origin().into(),
            certificate_sha256: qr.certificate_sha256().into(),
            registration_id: uuid::Uuid::new_v4().to_string(),
            paired: false,
        };
        let mut key = Credential::generate()?;
        let proof = key.enrollment(qr.enrollment_token(), device_name.trim())?;
        // reserve is CAS, so a losing concurrent enrollment never replaces keys.
        self.hosts.reserve(&pending)?;
        self.save_credential(&pending, &key)?;
        Self::check(&*self.data()?, generation)?;
        let value = tokio::select! {
            biased;
            _ = changed.changed() => return Err(ClientError::OutcomeUnknown),
            result = self.transport.enroll(&pending, &proof) => result?,
        };
        let reply: EnrollmentReply =
            serde_json::from_value(value).map_err(|_| ClientError::OutcomeUnknown)?;
        if reply.version != 1 || reply.host_id != pending.id || !uuid_id(&reply.device_id) {
            return Err(ClientError::OutcomeUnknown);
        }
        let data = self.data()?;
        Self::check(&data, generation).map_err(|_| ClientError::OutcomeUnknown)?;
        if self.hosts.load(&pending.id)?.as_ref() != Some(&pending) {
            return Err(ClientError::OutcomeUnknown);
        }
        key.device_id = Some(reply.device_id);
        self.save_credential(&pending, &key)
            .map_err(|_| ClientError::OutcomeUnknown)?;
        let mut paired = pending.clone();
        paired.paired = true;
        self.hosts
            .replace(&pending, &paired)
            .map_err(|_| ClientError::OutcomeUnknown)?;
        Ok(paired)
    }

    pub async fn capabilities(&self, host_id: &str) -> Result<crate::HostCapabilities> {
        let host = self.host(host_id)?;
        let key = self.credential(&host)?;
        let (value, generation) = self.request(host_id, HostAction::Capabilities).await?;
        let caps: crate::HostCapabilities =
            serde_json::from_value(value).map_err(|_| ClientError::InvalidResponse)?;
        if caps.version != 1
            || caps.host_id != host_id
            || key.device_id.as_deref() != Some(&caps.device_id)
        {
            self.invalidate()?;
            return Err(ClientError::Denied);
        }
        let mut data = self.data()?;
        Self::check(&data, generation)?;
        if !caps.session_observe_granted {
            self.invalidate_data(&mut data)?;
        }
        Ok(caps)
    }
}

pub(crate) fn now() -> Result<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| d.as_secs().try_into().ok())
        .ok_or(ClientError::InvalidInput)
}
fn short_label(label: &str) -> Result<()> {
    if label.trim().is_empty() || label.len() > 120 || label.chars().any(char::is_control) {
        Err(ClientError::InvalidInput)
    } else {
        Ok(())
    }
}
pub(crate) fn uuid_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}
pub(crate) fn credential_key(host: &Host) -> String {
    host.credential_storage_key()
}
pub(crate) fn validate_host(host: &Host) -> Result<()> {
    if !uuid_id(&host.id)
        || !uuid_id(&host.registration_id)
        || short_label(&host.label).is_err()
        || oqto_personal_wire::validate_origin(&host.origin)
            .ok()
            .as_ref()
            != Some(&host.origin)
        || oqto_personal_wire::tls::pinned_client_config(&host.certificate_sha256).is_err()
    {
        return Err(ClientError::InvalidInput);
    }
    Ok(())
}
