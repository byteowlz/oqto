use crate::history::bounded_text;
use crate::{
    Attachment, ClientError, ContextUsage, InputReceipt, Model, Observation, ObservationState,
    PersonalClient, Result, Session, SessionInput,
};
use oqto_personal_wire::actions::HostAction;
use serde_json::Value;
use std::collections::HashSet;
use tokio::time::{Duration, Instant};

pub(crate) struct Recovery {
    state: ObservationState,
    failures: u32,
    next: Instant,
    attachment: Option<String>,
}
impl Recovery {
    fn new() -> Self {
        Self {
            state: ObservationState::Connecting,
            failures: 0,
            next: Instant::now(),
            attachment: None,
        }
    }
    fn waiting(&self) -> Option<Observation> {
        let remaining = self.next.saturating_duration_since(Instant::now());
        (!remaining.is_zero()).then(|| Observation {
            state: self.state,
            attachment: None,
            retry_after_ms: remaining.as_millis().max(1) as u64,
            reconnected: false,
            history: None,
            history_pending: false,
        })
    }
}
impl PersonalClient {
    pub async fn catalog(&self, host: &str) -> Result<Vec<Session>> {
        let _gate = self.session_gate.lock().await;
        let row = self.host(host)?;
        let (value, generation) = self.request(host, HostAction::SessionsList).await?;
        let rows = value
            .get("sessions")
            .and_then(Value::as_array)
            .filter(|r| r.len() <= 5000)
            .ok_or(ClientError::InvalidResponse)?;
        let mut ids = HashSet::new();
        let mut sessions = Vec::new();
        for value in rows {
            let mut session: Session =
                serde_json::from_value(value.clone()).map_err(|_| ClientError::InvalidResponse)?;
            if !bounded_text(&session.id, 256)
                || !ids.insert(session.id.clone())
                || !bounded_text(&session.workspace, 4096)
            {
                return Err(ClientError::InvalidResponse);
            }
            session.title = session.title.filter(|s| s.len() <= 4096);
            session.display_title = session.display_title.filter(|s| bounded_text(s, 4096));
            session.workspace_root = session.workspace_root.filter(|s| bounded_text(s, 4096));
            session.activity_at_ms = session.activity_at_ms.filter(|ms| *ms > 0);
            sessions.push(session);
        }
        crate::relations::normalize(&mut sessions);
        let mut data = self.data()?;
        Self::check(&data, generation)?;
        if self.host(host)? != row {
            return Err(ClientError::Cancelled);
        }
        if data
            .catalogs
            .get(host)
            .is_some_and(|(registration, _)| registration != &row.registration_id)
        {
            data.attachments.retain(|(h, _), _| h != host);
            data.histories.retain(|(h, _), _| h != host);
            data.observations.retain(|(h, _), _| h != host);
        }
        data.catalogs
            .insert(host.into(), (row.registration_id, sessions.clone()));
        Ok(sessions)
    }

    pub async fn attach(&self, host: &str, session: &str) -> Result<Attachment> {
        let _gate = self.session_gate.lock().await;
        self.known_session(host, session)?;
        let existing = self
            .data()?
            .attachments
            .iter()
            .find(|((h, _), (_, a))| h == host && a.session_id == session)
            .map(|((_, id), _)| id.clone());
        if let Some(id) = existing
            && let Ok(entry) = self.read_locked(host, &id).await
        {
            return Ok(entry);
        }
        if self.data()?.attachments.len() >= 8 {
            return Err(ClientError::Denied);
        }
        let row = self.host(host)?;
        let (value, generation) = self
            .request(
                host,
                HostAction::SessionAttach {
                    session_id: session.into(),
                },
            )
            .await?;
        let id = text(&value, "attachment_id")?;
        if !crate::client::uuid_id(&id) || text(&value, "session_id")? != session {
            return Err(ClientError::InvalidResponse);
        }
        let attachment = Attachment {
            host_id: host.into(),
            session_id: session.into(),
            id,
            streaming: boolean(&value, "streaming")?,
            compacting: boolean(&value, "compacting")?,
            control_supported: value["control_supported"].as_bool() == Some(true),
            input_held: false,
            native_leaf_id: leaf(&value)?,
            preview_text: None,
            preview_tool: None,
            model: None,
            thinking_level: None,
            context_usage: None,
            queue_pending: None,
            queue_count: None,
        };
        let mut data = self.data()?;
        Self::check(&data, generation)?;
        let name = (host.into(), attachment.id.clone());
        if self.host(host)? != row || data.attachments.contains_key(&name) {
            return Err(ClientError::InvalidResponse);
        }
        data.attachments
            .insert(name, (row.registration_id, attachment.clone()));
        Ok(attachment)
    }

    fn handle(&self, host: &str, id: &str) -> Result<Attachment> {
        let row = self.host(host)?;
        let data = self.data()?;
        if !data.active {
            return Err(ClientError::Cancelled);
        }
        data.attachments
            .get(&(host.into(), id.into()))
            .filter(|(registration, _)| registration == &row.registration_id)
            .map(|(_, attachment)| attachment.clone())
            .ok_or(ClientError::Denied)
    }

    pub async fn read(&self, host: &str, id: &str) -> Result<Attachment> {
        let _gate = self.session_gate.lock().await;
        self.read_locked(host, id).await
    }
    async fn read_locked(&self, host: &str, id: &str) -> Result<Attachment> {
        let mut entry = self.handle(host, id)?;
        let name = (host.to_owned(), id.to_owned());
        let result = async {
            let (value, generation) = self
                .request(
                    host,
                    HostAction::SessionRead {
                        attachment_id: id.into(),
                    },
                )
                .await?;
            if text(&value, "session_id")? != entry.session_id {
                return Err(ClientError::InvalidResponse);
            }
            entry.streaming = boolean(&value, "streaming")?;
            entry.compacting = boolean(&value, "compacting")?;
            entry.input_held &= boolean(&value, "input_held")?;
            entry.native_leaf_id = leaf(&value)?;
            entry.preview_text = entry
                .streaming
                .then(|| preview(&value, "text", 300 * 1024))
                .flatten();
            entry.preview_tool = entry
                .streaming
                .then(|| preview(&value, "tool", 200))
                .flatten();
            entry.model = serde_json::from_value::<Model>(value["details"]["model"].clone())
                .ok()
                .filter(|m| {
                    bounded_text(&m.id, 512)
                        && bounded_text(&m.provider, 512)
                        && m.name.len() <= 512
                });
            entry.thinking_level = value["details"]["thinking_level"]
                .as_str()
                .filter(|s| bounded_text(s, 32))
                .map(str::to_owned);
            entry.context_usage = context_usage(&value["details"]["context_usage"]);
            entry.queue_pending = value["details"]["queue_pending"].as_bool();
            entry.queue_count = value["details"]["queue_count"]
                .as_u64()
                .and_then(|n| n.try_into().ok());
            let mut data = self.data()?;
            Self::check(&data, generation)?;
            let (_, current) = data
                .attachments
                .get_mut(&name)
                .ok_or(ClientError::Cancelled)?;
            *current = entry.clone();
            Ok(entry)
        }
        .await;
        if result.is_err() {
            self.data()?.attachments.remove(&name);
        }
        result
    }

    pub async fn request_control(&self, host: &str, id: &str) -> Result<Attachment> {
        let _gate = self.session_gate.lock().await;
        let mut entry = self.handle(host, id)?;
        if !entry.control_supported {
            return Err(ClientError::Denied);
        }
        let result = self
            .request(
                host,
                HostAction::SessionRequestControl {
                    attachment_id: id.into(),
                },
            )
            .await;
        let (value, generation) = match result {
            Ok(reply) => reply,
            Err(error) => {
                self.data()?.attachments.remove(&(host.into(), id.into()));
                return Err(error);
            }
        };
        if value["input_held"].as_bool() != Some(true) {
            self.data()?.attachments.remove(&(host.into(), id.into()));
            return Err(ClientError::Denied);
        }
        let mut data = self.data()?;
        Self::check(&data, generation)?;
        let (_, current) = data
            .attachments
            .get_mut(&(host.into(), id.into()))
            .ok_or(ClientError::Cancelled)?;
        entry.input_held = true;
        *current = entry.clone();
        Ok(entry)
    }

    pub async fn input(&self, host: &str, id: &str, input: SessionInput) -> Result<InputReceipt> {
        let _gate = self.session_gate.lock().await;
        let entry = self.handle(host, id)?;
        if !entry.input_held {
            return Err(ClientError::Denied);
        }
        validate_input(&input)?;
        let name = (host.to_owned(), id.to_owned());
        let (value, generation) = match self
            .request(
                host,
                HostAction::SessionInput {
                    attachment_id: id.into(),
                    input,
                },
            )
            .await
        {
            Ok(reply) => reply,
            Err(error) => {
                self.data()?.attachments.remove(&name);
                return Err(error);
            }
        };
        let receipt = match (
            text(&value, "public_session_id"),
            text(&value, "request_id"),
        ) {
            (Ok(session_id), Ok(request_id)) if session_id == entry.session_id => InputReceipt {
                session_id,
                request_id,
            },
            _ => {
                self.data()?.attachments.remove(&name);
                return Err(ClientError::OutcomeUnknown);
            }
        };
        let data = self.data()?;
        Self::check(&data, generation).map_err(|_| ClientError::OutcomeUnknown)?;
        if !data.attachments.contains_key(&name) {
            return Err(ClientError::OutcomeUnknown);
        }
        Ok(receipt)
    }

    pub async fn detach(&self, host: &str, id: &str) -> Result<()> {
        self.handle(host, id)?;
        // Invalidate before I/O, independently of a waiting control confirmation.
        self.data()?.attachments.remove(&(host.into(), id.into()));
        let (value, _) = self
            .request(
                host,
                HostAction::SessionDetach {
                    attachment_id: id.into(),
                },
            )
            .await?;
        if value["detached"].as_bool() != Some(true) {
            return Err(ClientError::OutcomeUnknown);
        }
        Ok(())
    }

    pub async fn observe(&self, host: &str, session: &str) -> Result<Observation> {
        let _gate = self.observation_gate.lock().await;
        self.known_session(host, session)?;
        let generation = self.data()?.generation;
        let name = (host.to_owned(), session.to_owned());
        if let Some(waiting) = self
            .data()?
            .observations
            .get(&name)
            .and_then(Recovery::waiting)
        {
            return Ok(waiting);
        }
        let existing = self
            .data()?
            .attachments
            .iter()
            .find(|((h, _), (_, a))| h == host && a.session_id == session)
            .map(|((_, id), _)| id.clone());
        let reattached = existing.is_none();
        let result = match existing {
            Some(id) => self.read(host, &id).await,
            None => match self.attach(host, session).await {
                Ok(attachment) => self.read(host, &attachment.id).await,
                Err(error) => Err(error),
            },
        };
        let attachment = match result {
            Ok(attachment) => attachment,
            Err(
                error @ (ClientError::Denied | ClientError::PinMismatch | ClientError::Cancelled),
            ) => return Err(error),
            Err(error) => {
                let mut data = self.data()?;
                Self::check(&data, generation)?;
                let recovery = data.observations.entry(name).or_insert_with(Recovery::new);
                recovery.state = if error == ClientError::NotReady {
                    ObservationState::Synchronizing
                } else {
                    ObservationState::Disconnected
                };
                recovery.attachment = None;
                let seconds = (2u64 << recovery.failures.min(4)).min(30);
                recovery.failures = recovery.failures.saturating_add(1);
                recovery.next = Instant::now() + Duration::from_secs(seconds);
                return recovery.waiting().ok_or(ClientError::InvalidResponse);
            }
        };
        let reconnected = reattached
            || self.data()?.observations.get(&name).is_none_or(|r| {
                r.state != ObservationState::Connected
                    || r.attachment.as_deref() != Some(&attachment.id)
            });
        let history = if reconnected {
            match self.history(host, session, None).await {
                Ok(page) => Some(page),
                Err(
                    error @ (ClientError::Denied
                    | ClientError::PinMismatch
                    | ClientError::Cancelled),
                ) => return Err(error),
                Err(_) => None,
            }
        } else {
            None
        };
        let mut data = self.data()?;
        Self::check(&data, generation)?;
        let mut recovery = Recovery::new();
        recovery.state = ObservationState::Connected;
        recovery.attachment = Some(attachment.id.clone());
        data.observations.insert(name, recovery);
        Ok(Observation {
            state: ObservationState::Connected,
            attachment: Some(attachment),
            retry_after_ms: 0,
            reconnected,
            history_pending: reconnected && history.is_none(),
            history,
        })
    }
}

fn text(value: &Value, field: &str) -> Result<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| bounded_text(s, 4096))
        .map(str::to_owned)
        .ok_or(ClientError::InvalidResponse)
}
fn boolean(value: &Value, field: &str) -> Result<bool> {
    value
        .get(field)
        .and_then(Value::as_bool)
        .ok_or(ClientError::InvalidResponse)
}
fn leaf(value: &Value) -> Result<Option<String>> {
    match value.get("native_leaf_id") {
        Some(Value::Null) => Ok(None),
        Some(Value::String(id)) if bounded_text(id, 4096) => Ok(Some(id.clone())),
        _ => Err(ClientError::InvalidResponse),
    }
}
fn preview(value: &Value, field: &str, limit: usize) -> Option<String> {
    value["preview"][field]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= limit)
        .map(str::to_owned)
}
fn context_usage(value: &Value) -> Option<ContextUsage> {
    let tokens = match value.get("tokens")? {
        Value::Null => None,
        tokens => Some(tokens.as_u64()?),
    };
    Some(ContextUsage {
        tokens,
        context_window: value["context_window"].as_u64().filter(|w| *w > 0)?,
    })
}
fn validate_input(input: &SessionInput) -> Result<()> {
    let valid = match input {
        SessionInput::Prompt { message }
        | SessionInput::Steer { message }
        | SessionInput::FollowUp { message } => {
            !message.trim().is_empty() && message.len() <= 32 * 1024
        }
        SessionInput::SetModel { provider, model_id } => {
            bounded_text(provider, 512) && bounded_text(model_id, 512)
        }
        SessionInput::SetThinkingLevel { level } => bounded_text(level, 32),
        SessionInput::Abort => true,
    };
    if valid {
        Ok(())
    } else {
        Err(ClientError::InvalidInput)
    }
}
