//! Canonical page validation and opaque cursor fences, not a UI timeline.
use crate::{ClientError, HistoryPage, NativeHead, PersonalClient, Result};
use oqto_personal_wire::actions::HostAction;
use serde_json::Value;
use std::collections::HashSet;

pub(crate) struct Paging {
    next: Option<String>,
    head: NativeHead,
    seen: HashSet<String>,
}
impl PersonalClient {
    pub async fn history(
        &self,
        host: &str,
        session: &str,
        before: Option<String>,
    ) -> Result<HistoryPage> {
        let _gate = self.history_gate.lock().await;
        self.known_session(host, session)?;
        let name = (host.to_owned(), session.to_owned());
        if let Some(cursor) = before.as_deref()
            && !self
                .data()?
                .histories
                .get(&name)
                .is_some_and(|p| p.next.as_deref() == Some(cursor) && !p.seen.contains(cursor))
        {
            return Err(ClientError::InvalidInput);
        }
        let (value, generation) = self
            .request(
                host,
                HostAction::SessionHistory {
                    session_id: session.into(),
                    before: before.clone(),
                },
            )
            .await?;
        let page = parse_page(value, session)?;
        let mut data = self.data()?;
        Self::check(&data, generation)?;
        if let Some(cursor) = before {
            let previous = data
                .histories
                .get_mut(&name)
                .ok_or(ClientError::Cancelled)?;
            let same_head = if cursor.starts_with("native:") {
                previous.head.branch_id == page.native_graph.branch_id
            } else {
                previous.head == page.native_graph
            };
            if !same_head
                || previous.next.as_ref() != Some(&cursor)
                || !previous.seen.insert(cursor)
            {
                data.histories.remove(&name);
                return Err(ClientError::InvalidResponse);
            }
            if page
                .next_before
                .as_ref()
                .is_some_and(|next| previous.seen.contains(next))
            {
                data.histories.remove(&name);
                return Err(ClientError::InvalidResponse);
            }
            previous.next = page.next_before.clone();
            previous.head = page.native_graph.clone();
        } else {
            data.histories.insert(
                name,
                Paging {
                    next: page.next_before.clone(),
                    head: page.native_graph.clone(),
                    seen: HashSet::new(),
                },
            );
        }
        Ok(page)
    }
}

pub(crate) fn bounded_text(value: &str, limit: usize) -> bool {
    !value.is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
}

pub(crate) fn parse_page(value: Value, session: &str) -> Result<HistoryPage> {
    let graph = value
        .get("native_graph")
        .filter(|g| g.is_object())
        .ok_or(ClientError::NotReady)?;
    if !matches!(
        graph.get("native_leaf_id"),
        Some(Value::Null | Value::String(_))
    ) || serde_json::to_vec(graph)
        .map_err(|_| ClientError::InvalidResponse)?
        .len()
        > 8192
    {
        return Err(ClientError::InvalidResponse);
    }
    let page: HistoryPage =
        serde_json::from_value(value).map_err(|_| ClientError::InvalidResponse)?;
    let head = &page.native_graph;
    if head.source != "pi-native-graph-v1"
        || !bounded_text(&head.branch_id, 256)
        || head
            .native_leaf_id
            .as_ref()
            .is_some_and(|id| !bounded_text(id, 4096))
        || page.messages.len() > 1000
        || (page.has_more
            && page
                .next_before
                .as_ref()
                .is_none_or(|c| !bounded_text(c, 2048)))
        || (!page.has_more && page.next_before.is_some())
    {
        return Err(ClientError::InvalidResponse);
    }
    let mut messages = HashSet::new();
    let mut parts = HashSet::new();
    for message in &page.messages {
        if message.session_id != session
            || !bounded_text(&message.id, 4096)
            || !messages.insert(&message.id)
            || !bounded_text(&message.role, 64)
        {
            return Err(ClientError::InvalidResponse);
        }
        for part in &message.parts {
            if !bounded_text(&part.id, 4096)
                || !bounded_text(&part.part_type, 128)
                || !parts.insert(&part.id)
            {
                return Err(ClientError::InvalidResponse);
            }
        }
    }
    Ok(page)
}
