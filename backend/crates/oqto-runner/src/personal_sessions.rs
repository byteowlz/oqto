//! Device-bound handles to existing terminal-owned Pi processes. No spawn,
//! transcript ingestion or native session-file write is available here.
use crate::tui_rpc::{
    ControlAuthority, ControlPermit, ObservationAuthority, ObservationBinding, TuiObserver,
};
use anyhow::{Context, Result, ensure};
use oqto_personal::actions::ActionFailure;
use oqto_personal::{AuthorizedAction, DeviceFence};
use oqto_personal_wire::actions::{HostAction, SessionInput};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const IDLE: Duration = Duration::from_secs(60);
const MAX_ATTACHMENTS: usize = 128;
const MAX_DEVICE_ATTACHMENTS: usize = 8;

struct Authority {
    binding: ObservationBinding,
    fence: DeviceFence,
    permit: Option<ControlPermit>,
    source: Option<(Arc<crate::personal_source::PersonalSource>, String)>,
}
impl ObservationAuthority for Authority {
    fn resolve(&self, authority: &str, public_session: &str) -> Result<ObservationBinding> {
        ensure!(
            authority == self.binding.authority_id
                && public_session == self.binding.public_session_id,
            "Session identity denied"
        );
        self.revalidate(&self.binding)?;
        Ok(self.binding.clone())
    }
    fn revalidate(&self, binding: &ObservationBinding) -> Result<()> {
        ensure!(
            self.fence.is_current()
                && binding.authority_id == self.binding.authority_id
                && binding.public_session_id == self.binding.public_session_id
                && binding.external_session_id == self.binding.external_session_id
                && binding.grant_generation == self.fence.generation(),
            "Session grant ended"
        );
        if let Some((source, identity)) = &self.source {
            ensure!(
                source.header(&binding.session_file)?.identity == *identity,
                "Native source replaced"
            );
        }
        Ok(())
    }
}
impl ControlAuthority for Authority {
    fn authorize_control(
        &self,
        binding: &ObservationBinding,
        controller: &str,
    ) -> Result<ControlPermit> {
        self.revalidate(binding)?;
        ensure!(
            controller == self.fence.device_id(),
            "Device identity denied"
        );
        self.permit.clone().context("Control not granted")
    }
    fn revalidate_control(
        &self,
        binding: &ObservationBinding,
        permit: &ControlPermit,
    ) -> Result<()> {
        self.revalidate(binding)?;
        ensure!(
            permit.controller_id() == self.fence.device_id()
                && permit.generation() == self.fence.generation(),
            "Control generation changed"
        );
        Ok(())
    }
}
type DeviceKey = (String, String, String);
#[derive(Default)]
struct Budget(Mutex<HashMap<DeviceKey, usize>>);
struct Reservation {
    budget: Arc<Budget>,
    key: DeviceKey,
}
impl Budget {
    fn reserve(self: &Arc<Self>, key: DeviceKey) -> Result<Reservation> {
        let mut counts = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Attachment budget unavailable"))?;
        ensure!(
            counts.values().sum::<usize>() < MAX_ATTACHMENTS
                && counts.get(&key).copied().unwrap_or(0) < MAX_DEVICE_ATTACHMENTS,
            "Attachment limit reached"
        );
        *counts.entry(key.clone()).or_default() += 1;
        Ok(Reservation {
            budget: self.clone(),
            key,
        })
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if let Ok(mut counts) = self.budget.0.lock()
            && let Some(count) = counts.get_mut(&self.key)
        {
            *count = count.saturating_sub(1);
            if *count == 0 {
                counts.remove(&self.key);
            }
        }
    }
}
struct Attachment {
    _reservation: Reservation,
    observer: TuiObserver,
    binding: ObservationBinding,
    user_id: String,
    fence: DeviceFence,
    touched: Instant,
    source: Option<(Arc<crate::personal_source::PersonalSource>, String)>,
}
impl Attachment {
    fn belongs_to(&self, fence: &DeviceFence) -> bool {
        self.fence.host_id() == fence.host_id()
            && self.fence.device_id() == fence.device_id()
            && self.fence.generation() == fence.generation()
    }
    fn current(&self) -> bool {
        self.fence.is_current() && self.touched.elapsed() < IDLE
    }
}
pub(crate) struct Sessions {
    handles: Mutex<HashMap<String, Attachment>>,
    budget: Arc<Budget>,
    socket_root: PathBuf,
    ingest: Option<crate::daemon::jsonl_ingest::IngestHandle>,
    source: Option<Arc<crate::personal_source::PersonalSource>>,
}
impl Default for Sessions {
    fn default() -> Self {
        Self {
            handles: Mutex::default(),
            budget: Arc::default(),
            socket_root: std::env::temp_dir(),
            ingest: None,
            source: None,
        }
    }
}
impl Sessions {
    pub(crate) fn with_ingest(ingest: crate::daemon::jsonl_ingest::IngestHandle) -> Self {
        Self {
            socket_root: ingest
                .source
                .as_ref()
                .map(|source| source.socket_root().to_owned())
                .unwrap_or_else(std::env::temp_dir),
            source: ingest.source.clone(),
            ingest: Some(ingest),
            ..Self::default()
        }
    }
    fn nudge(
        &self,
        attachment: &Attachment,
        state: &crate::tui_rpc::ObservedState,
    ) -> Result<bool> {
        state.head.leaf_id()?;
        Ok(self.ingest.as_ref().is_some_and(|ingest| {
            ingest.nudge_native(crate::daemon::jsonl_ingest::NativeObservation {
                binding: attachment.binding.clone(),
                user_id: attachment.user_id.clone(),
                fence: attachment.fence.clone(),
            })
        }))
    }
    pub(crate) fn reap(&self) {
        if let Ok(mut handles) = self.handles.lock() {
            handles.retain(|_, attachment| attachment.current());
        }
    }
    /// Resolve canonical ownership and the immutable native source before any
    /// socket discovery. This path only reads; the writer bootstraps identities.
    async fn resolve_authority(
        &self,
        home: &Path,
        roots: &[PathBuf],
        session_id: &str,
        admission: &AuthorizedAction,
    ) -> Result<(Arc<Authority>, String)> {
        let row = crate::history_read::resolve_authorized_session(home, roots, session_id).await?;
        let workspace = PathBuf::from(row.workspace_id.context("Workspace unavailable")?);
        let workspace_root = roots
            .iter()
            .find(|root| workspace.starts_with(root))
            .context("Workspace denied")?
            .clone();
        let external = row.external_id.context("Native identity unavailable")?;
        if let Some(source) = &self.source {
            ensure!(
                source.home == home && source.workspace_roots() == roots,
                "Personal source mismatch"
            );
            source.validate()?;
        }
        let session_root = self
            .source
            .as_ref()
            .map(|source| source.session_root().to_owned())
            .unwrap_or_else(|| home.join(".pi/agent/sessions"));
        let session_file = oqto_pi::session_files::find_unique_session_file_in(
            &session_root,
            &external,
            &workspace,
        )?
        .context("Native source unavailable")?;
        let fence = admission.fence();
        let source_identity = self
            .source
            .as_ref()
            .map(|source| {
                source
                    .header(&session_file)
                    .map(|header| (source.clone(), header.identity))
            })
            .transpose()?;
        let authority = Arc::new(Authority {
            binding: ObservationBinding {
                authority_id: format!("personal-host:{}", fence.host_id()),
                public_session_id: session_id.to_owned(),
                external_session_id: external,
                session_file,
                session_root,
                work_directory: workspace,
                workspace_root,
                socket_root: self.socket_root.canonicalize()?,
                grant_generation: fence.generation(),
            },
            permit: None,
            fence: fence.clone(),
            source: source_identity,
        });
        if let Some(source) = &self.source {
            source.validate_binding(&authority.binding).await?;
        }
        Ok((authority, row.user_id))
    }
    async fn attach(
        &self,
        home: &Path,
        roots: &[PathBuf],
        session_id: String,
        admission: &AuthorizedAction,
    ) -> Result<Value> {
        self.reap();
        let fence = admission.fence();
        let reservation = self.budget.reserve((
            fence.host_id().into(),
            fence.device_id().into(),
            fence.generation(),
        ))?;
        let (authority, user_id) = self
            .resolve_authority(home, roots, &session_id, admission)
            .await?;
        let observer = TuiObserver::attach_snapshots(
            authority.clone(),
            &authority.binding.authority_id,
            &session_id,
        )
        .await?;
        let state = observer.initial_state();
        let native_leaf_id = state.head.leaf_id()?;
        let control_supported = observer.control_supported();
        let id = uuid::Uuid::new_v4().to_string();
        let mut handles = self
            .handles
            .lock()
            .map_err(|_| anyhow::anyhow!("Session registry unavailable"))?;
        handles.retain(|_, attachment| attachment.current());
        ensure!(
            fence.is_current()
                && handles.len() < MAX_ATTACHMENTS
                && handles
                    .values()
                    .filter(|attachment| attachment.belongs_to(&fence))
                    .count()
                    < MAX_DEVICE_ATTACHMENTS,
            "Attachment limit or revoked grant"
        );
        let attachment = Attachment {
            _reservation: reservation,
            observer,
            binding: authority.binding.clone(),
            user_id,
            fence,
            touched: Instant::now(),
            source: authority.source.clone(),
        };
        let sync_requested = self.nudge(&attachment, &state)?;
        handles.insert(id.clone(), attachment);
        Ok(
            json!({"attachment_id": id, "session_id": session_id, "streaming": state.streaming, "compacting": state.compacting, "control_supported": control_supported, "containment_verified": false, "idle_timeout_seconds": IDLE.as_secs(), "native_leaf_id": native_leaf_id, "sync_requested": sync_requested}),
        )
    }
    pub(crate) async fn execute(
        &self,
        home: &Path,
        roots: &[PathBuf],
        action: HostAction,
        admission: AuthorizedAction,
    ) -> Result<Value, ActionFailure> {
        if let HostAction::SessionAttach { session_id } = action {
            return self
                .attach(home, roots, session_id, &admission)
                .await
                .map_err(|error| {
                    if (oqto_history::oqto_log::ops::native_source::authority_unavailable(&error)
                        || error.chain().any(|cause| cause.is::<sqlx::Error>()))
                        && let Some(ingest) = &self.ingest
                    {
                        ingest.mark_authority_failed();
                    }
                    ActionFailure::Unavailable
                });
        }
        let id = match &action {
            HostAction::SessionRead { attachment_id }
            | HostAction::SessionRequestControl { attachment_id }
            | HostAction::SessionReleaseControl { attachment_id }
            | HostAction::SessionInput { attachment_id, .. }
            | HostAction::SessionDetach { attachment_id } => attachment_id,
            _ => return Err(ActionFailure::Invalid),
        }
        .clone();
        // Remove before awaiting: cancellation or an uncertain native outcome
        // drops OUR connection. It cannot leave a half-frame reusable handle.
        let mut attachment = {
            let mut handles = self
                .handles
                .lock()
                .map_err(|_| ActionFailure::Unavailable)?;
            let entry = handles.get(&id).ok_or(ActionFailure::NotFound)?;
            if !entry.belongs_to(&admission.fence()) {
                return Err(ActionFailure::Denied);
            }
            handles.remove(&id).ok_or(ActionFailure::NotFound)?
        };
        if !attachment.current() {
            return Err(ActionFailure::Denied);
        }
        let result = match action {
            HostAction::SessionRead { .. } => {
                let state = attachment.observer.snapshot_state().await.map_err(|_| ActionFailure::Unavailable)?;
                let sync_requested = self.nudge(&attachment, &state).map_err(|_| ActionFailure::Unavailable)?;
                Ok(json!({"session_id": attachment.observer.public_session_id(), "streaming": state.streaming, "compacting": state.compacting, "input_held": attachment.observer.input_held(), "containment_verified": false, "native_leaf_id": state.head.leaf_id().map_err(|_| ActionFailure::Unavailable)?, "sync_requested": sync_requested}))
            }
            HostAction::SessionRequestControl { .. } => {
                let permit = ControlPermit::from_paired_device(&admission, attachment.observer.public_session_id().into()).map_err(|_| ActionFailure::Denied)?;
                let binding = attachment.binding.clone();
                let authority = Arc::new(Authority { binding, fence: admission.fence(), permit: Some(permit), source: attachment.source.clone() });
                attachment.observer.acquire_input(authority, admission.fence().device_id()).await.map(|()| json!({"input_held": true}))
            }
            HostAction::SessionInput { input, .. } => {
                let input = match input {
                    SessionInput::Prompt { message } => crate::tui_rpc::InputAction::Prompt { message },
                    SessionInput::Steer { message } => crate::tui_rpc::InputAction::Steer { message },
                    SessionInput::Abort => crate::tui_rpc::InputAction::Abort,
                };
                let receipt = attachment.observer.input(input).await.map_err(|_| ActionFailure::OutcomeUnknown)?;
                Ok(serde_json::to_value(receipt).map_err(|_| ActionFailure::OutcomeUnknown)?)
            }
            HostAction::SessionReleaseControl { .. } | HostAction::SessionDetach { .. } => {
                attachment.observer.release_input();
                return Ok(json!({"detached": true}));
            }
            _ => return Err(ActionFailure::Invalid),
        }.map_err(|_| ActionFailure::Unavailable)?;
        if !attachment.fence.is_current() {
            return Err(ActionFailure::Denied);
        }
        attachment.touched = Instant::now();
        self.handles
            .lock()
            .map_err(|_| ActionFailure::Unavailable)?
            .insert(id.clone(), attachment);
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
