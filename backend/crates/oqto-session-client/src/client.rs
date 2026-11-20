//! The sans-IO session client: one Session's observable state, fed with host frames, producing
//! client frames. The async runtime ([`crate::runtime`]) moves frames; tests replay them.
//!
//! Behaviour follows the contract's caching rules and byoagnt's TypeScript client, which is
//! tested against the same bundle: fast start from the cache, `from` = newest held + 1, gaps
//! filled ascending, earlier entries descending, instant send with resend after reconnect,
//! purge on `denied` and `unknown_session`, cache dropped on a new `store`.

use std::collections::{BTreeMap, HashMap};

use oqto_session_view::frames::{
    ClientFrame, EndedReason, ErrorCode, Input, InputKind, Order, ReceiptKind, VERSION, Window,
    WriteEntry,
};
use oqto_session_view::{Entry, Gap, HostFrame, Receipt, ReceiptStatus, Row, Transcript, View};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cache::CacheRecord;

/// What a request asks for, without connection ids; kept and resent unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// User input.
    Submit {
        /// Input kind and text.
        input: Input,
    },
    /// A passive entry.
    Write {
        /// Kind and data.
        entry: WriteEntry,
    },
    /// Stop the run and withdraw queued inputs.
    Abort,
}

impl Request {
    /// Whether it is shown in the transcript until its entry arrives.
    pub const fn shown(&self) -> bool {
        matches!(self, Self::Submit { .. } | Self::Write { .. })
    }
}

/// Local request provenance, not part of the wire contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RequestSource {
    /// Queued before any store was observed; safe to bind on first attachment.
    Unsent,
    /// Intended for this store, even when queued offline.
    Store {
        /// Immutable storage incarnation, not the public Session id.
        store: String,
    },
    /// The store changed, or an old cache has no proven source. Never auto-replayed.
    Uncertain {
        /// Original store when known. Its outcome is not inferred from a replacement.
        store: Option<String>,
    },
}

impl RequestSource {
    fn for_store(store: Option<&str>) -> Self {
        store.map_or(Self::Unsent, |store| Self::Store {
            store: store.to_owned(),
        })
    }

    fn bind(&mut self, current: &str) {
        match self {
            Self::Unsent => *self = Self::for_store(Some(current)),
            Self::Store { store } if store != current => {
                *self = Self::Uncertain {
                    store: Some(store.clone()),
                };
            }
            _ => {}
        }
    }

    fn matches(&self, current: Option<&str>) -> bool {
        matches!(self, Self::Store { store } if Some(store.as_str()) == current)
    }
}

/// A request this client made and still tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tracked {
    /// Idempotency key.
    pub request_id: String,
    /// What it asks for.
    pub request: Request,
    /// Its receipt is final.
    #[serde(default)]
    pub settled: bool,
    /// Original store binding. Missing only in older caches; migrated on restore.
    #[serde(default)]
    pub source: Option<RequestSource>,
}

/// One row to render: the transcript's rows, then pending sends.
#[derive(Debug, Clone, PartialEq)]
pub enum Item<'a> {
    /// Older entries may exist.
    Earlier,
    /// A committed entry.
    Entry(&'a Entry),
    /// Missing entries.
    Gap(Gap),
    /// Shown at once under its request id until its entry is held.
    Pending {
        /// The request.
        tracked: &'a Tracked,
        /// Its latest receipt.
        receipt: Option<&'a Receipt>,
    },
}

/// Something the user should see once (status line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// The host refused one of this client's requests.
    Refused {
        /// The error code.
        code: String,
    },
    /// The cache was purged because access ended.
    Purged,
    /// A frame or view could not be applied; the client attached again.
    Resynced,
}

#[derive(Debug, Clone)]
enum Read {
    Gap {
        gap: Gap,
        known: u64,
        max: u64,
        cursor: Option<String>,
        restarted: bool,
    },
    Earlier {
        low: u64,
        max: u64,
        cursor: Option<String>,
    },
}

#[derive(Debug, Clone)]
struct Scan {
    max: u64,
    cursor: String,
    low: Option<u64>,
}

/// Construction options.
#[derive(Debug, Clone)]
pub struct Options {
    /// Window size asked for on attach.
    pub window_min: Option<u64>,
    /// Window maximum asked for on attach.
    pub window_max: Option<u64>,
    /// `read_entries` limit, 1 to 200.
    pub page_size: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            window_min: None,
            window_max: None,
            page_size: 50,
        }
    }
}

/// One Session's client state.
#[derive(Debug)]
pub struct SessionClient {
    session: String,
    options: Options,
    counter: u64,
    outgoing: Vec<ClientFrame>,
    connected: bool,
    attach_id: Option<String>,
    attachment: Option<String>,
    view: View,
    retired: bool,
    cached: bool,
    transcript: Transcript,
    store: Option<String>,
    requests: Vec<Tracked>,
    receipts: BTreeMap<String, Receipt>,
    correlation: HashMap<String, String>,
    reads: HashMap<String, Read>,
    earlier_scan: Option<Scan>,
    error: Option<String>,
    dirty: bool,
    purge: bool,
    notices: Vec<Notice>,
}

const fn terminal(status: ReceiptStatus) -> bool {
    status.is_final()
}

const fn definitive(code: ErrorCode) -> bool {
    !matches!(code, ErrorCode::Unavailable | ErrorCode::UnknownRequest)
}

const fn purging(code: ErrorCode) -> bool {
    matches!(code, ErrorCode::Denied | ErrorCode::UnknownSession)
}

impl SessionClient {
    /// A client for `session`.
    pub fn new(session: impl Into<String>, options: Options) -> Self {
        let mut options = options;
        options.page_size = options.page_size.clamp(1, 200);
        Self {
            session: session.into(),
            options,
            counter: 0,
            outgoing: Vec::new(),
            connected: false,
            attach_id: None,
            attachment: None,
            view: View::new(),
            retired: false,
            cached: false,
            transcript: Transcript::new(),
            store: None,
            requests: Vec::new(),
            receipts: BTreeMap::new(),
            correlation: HashMap::new(),
            reads: HashMap::new(),
            earlier_scan: None,
            error: None,
            dirty: false,
            purge: false,
            notices: Vec::new(),
        }
    }

    // ----- reading --------------------------------------------------------------------

    /// The Session id.
    pub fn session(&self) -> &str {
        &self.session
    }

    /// A transport is bound.
    pub const fn connected(&self) -> bool {
        self.connected
    }

    /// The live attachment, once `attached` arrived.
    pub fn attachment(&self) -> Option<&str> {
        self.attachment.as_deref()
    }

    /// The exact wire view (never merged with cached entries).
    pub const fn view(&self) -> &View {
        &self.view
    }

    /// What is shown comes from the cache; no live base has arrived yet.
    pub const fn cached(&self) -> bool {
        self.cached
    }

    /// Upstream retired the observed value.
    pub const fn retired(&self) -> bool {
        self.retired
    }

    /// The transcript held from views, pages and the cache.
    pub const fn transcript(&self) -> &Transcript {
        &self.transcript
    }

    /// The `store` named by `attached` or the cache.
    pub fn store(&self) -> Option<&str> {
        self.store.as_deref()
    }

    /// The latest error code, if any.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The latest receipt of a request.
    pub fn receipt(&self, request_id: &str) -> Option<&Receipt> {
        self.receipts.get(request_id)
    }

    /// The receipt that names `entry` as its input or write, if this client heard of it.
    pub fn receipt_for_entry(&self, entry: u64) -> Option<&Receipt> {
        self.receipts.values().find(|r| r.entry == Some(entry))
    }

    /// Requests still tracked, in the order they were made.
    pub fn requests(&self) -> &[Tracked] {
        &self.requests
    }

    /// Pending operations whose original outcome cannot be reconciled on this store.
    /// They remain cached with source and payload, but are never resent or newly settled here.
    pub fn uncertain_requests(&self) -> Vec<&Tracked> {
        self.requests
            .iter()
            .filter(|t| !t.settled && matches!(t.source, Some(RequestSource::Uncertain { .. })))
            .collect()
    }

    /// A gap fill or earlier load is running.
    pub fn loading(&self) -> (bool, usize) {
        let earlier = self
            .reads
            .values()
            .any(|r| matches!(r, Read::Earlier { .. }));
        let gaps = self
            .reads
            .values()
            .filter(|r| matches!(r, Read::Gap { .. }))
            .count();
        (earlier, gaps)
    }

    /// Rows to render: transcript rows, then pending sends.
    pub fn items(&self) -> Vec<Item<'_>> {
        let mut items: Vec<Item<'_>> = self
            .transcript
            .rows()
            .into_iter()
            .map(|row| match row {
                Row::Earlier => Item::Earlier,
                Row::Entry(entry) => Item::Entry(entry),
                Row::Gap(gap) => Item::Gap(gap),
            })
            .collect();
        for tracked in &self.requests {
            if tracked.request.shown() && !self.landed(&tracked.request_id) {
                items.push(Item::Pending {
                    tracked,
                    receipt: self.receipts.get(&tracked.request_id),
                });
            }
        }
        items
    }

    /// Frames to send, in order.
    pub fn take_outgoing(&mut self) -> Vec<ClientFrame> {
        std::mem::take(&mut self.outgoing)
    }

    /// Notices since the last call.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    /// Whether the cache should be written; clears the flag.
    pub const fn take_dirty(&mut self) -> bool {
        let dirty = self.dirty;
        self.dirty = false;
        dirty
    }

    /// Whether the cache must be deleted; clears the flag.
    pub const fn take_purge(&mut self) -> bool {
        let purge = self.purge;
        self.purge = false;
        purge
    }

    // ----- cache ----------------------------------------------------------------------

    /// Fast start: show a cached record before any frame arrives.
    pub fn restore(&mut self, record: CacheRecord) {
        if self.transcript.begun() {
            return;
        }
        self.transcript = Transcript::from_record(record.transcript);
        self.store = record.store;
        let live = std::mem::take(&mut self.requests);
        self.requests = record.requests;
        for tracked in &mut self.requests {
            tracked.source = Some(match tracked.source.take() {
                Some(RequestSource::Unsent) => RequestSource::Uncertain { store: None },
                Some(source) => source,
                None => self
                    .store
                    .as_deref()
                    .map_or(RequestSource::Uncertain { store: None }, |store| {
                        RequestSource::for_store(Some(store))
                    }),
            });
        }
        for tracked in live {
            self.requests.retain(|t| t.request_id != tracked.request_id);
            self.requests.push(tracked);
        }
        for (id, receipt) in record.receipts {
            self.receipts.entry(id).or_insert(receipt);
        }
        for tracked in &mut self.requests {
            if self
                .receipts
                .get(&tracked.request_id)
                .is_some_and(|r| terminal(r.status))
            {
                tracked.settled = true;
            }
        }
        self.prune();
        self.cached = !self.transcript.entries().is_empty() || !self.requests.is_empty();
    }

    /// The record to cache now.
    pub fn record(&self) -> CacheRecord {
        CacheRecord {
            v: 1,
            store: self.store.clone(),
            transcript: self.transcript.to_record(),
            requests: self.requests.clone(),
            receipts: self.receipts.clone(),
        }
    }

    // ----- connection -----------------------------------------------------------------

    /// Bind a new authenticated connection. Effects wait for the matching `attached`
    /// response and its store validation; connecting alone never replays a request.
    pub fn connect(&mut self) {
        self.disconnect();
        self.connected = true;
        self.attach();
    }

    /// The connection ended: its attachment and reads end silently.
    pub fn disconnect(&mut self) {
        self.connected = false;
        self.attach_id = None;
        self.attachment = None;
        self.correlation.clear();
        self.reads.clear();
        self.outgoing.clear();
    }

    fn next_id(&mut self) -> String {
        self.counter += 1;
        format!("c_{}", self.counter)
    }

    fn send(&mut self, frame: ClientFrame) {
        if self.connected {
            self.outgoing.push(frame);
        }
    }

    fn attach(&mut self) {
        if self.attach_id.is_some() || !self.connected {
            return;
        }
        if self.attachment.is_some() {
            self.detach();
        }
        let from = self.transcript.newest().map(|n| n + 1);
        let window = Window {
            from,
            min: self.options.window_min,
            max: self.options.window_max,
        };
        let window = (window != Window::default()).then_some(window);
        let id = self.next_id();
        self.attach_id = Some(id.clone());
        self.send(ClientFrame::Attach {
            id,
            v: VERSION,
            session: self.session.clone(),
            window,
        });
    }

    /// Stop the current attachment (the connection stays).
    pub fn detach(&mut self) {
        if let Some(attachment) = self.attachment.take() {
            let id = self.next_id();
            self.send(ClientFrame::Detach { id, attachment });
        }
    }

    fn resync(&mut self) {
        self.notices.push(Notice::Resynced);
        self.detach();
        self.attach();
    }

    // ----- effects --------------------------------------------------------------------

    /// User input; shown at once, sent now or on connect. Returns the request id.
    pub fn submit(&mut self, kind: InputKind, text: impl Into<String>) -> String {
        self.effect(Request::Submit {
            input: Input {
                kind,
                text: text.into(),
            },
        })
    }

    /// A passive entry the model never sees.
    pub fn write(&mut self, kind: impl Into<String>, data: Option<Value>) -> String {
        self.effect(Request::Write {
            entry: WriteEntry {
                kind: kind.into(),
                data,
            },
        })
    }

    /// Stop the run and withdraw queued inputs.
    pub fn abort(&mut self) -> String {
        self.effect(Request::Abort)
    }

    fn effect(&mut self, request: Request) -> String {
        let request_id = format!("req-{}", uuid::Uuid::new_v4());
        self.error = None;
        self.requests.push(Tracked {
            request_id: request_id.clone(),
            request,
            settled: false,
            source: Some(RequestSource::for_store(self.store.as_deref())),
        });
        self.resend(&request_id);
        self.dirty = true;
        request_id
    }

    fn frame_for(&mut self, tracked: &Tracked) -> ClientFrame {
        let id = self.next_id();
        let session = self.session.clone();
        let request_id = tracked.request_id.clone();
        match &tracked.request {
            Request::Submit { input } => ClientFrame::Submit {
                id,
                session,
                request_id,
                input: input.clone(),
            },
            Request::Write { entry } => ClientFrame::Write {
                id,
                session,
                request_id,
                entry: entry.clone(),
            },
            Request::Abort => ClientFrame::Abort {
                id,
                session,
                request_id,
            },
        }
    }

    fn resend(&mut self, request_id: &str) {
        let Some(tracked) = self
            .requests
            .iter()
            .find(|t| t.request_id == request_id && !t.settled)
            .cloned()
        else {
            return;
        };
        if !self.connected
            || self.attachment.is_none()
            || !tracked
                .source
                .as_ref()
                .is_some_and(|s| s.matches(self.store.as_deref()))
        {
            return;
        }
        let frame = self.frame_for(&tracked);
        self.correlation
            .insert(frame.id().to_owned(), tracked.request_id);
        self.send(frame);
    }

    fn query(&mut self, request_id: &str) {
        if self.attachment.is_none()
            || self.requests.iter().any(|t| {
                t.request_id == request_id
                    && !t
                        .source
                        .as_ref()
                        .is_some_and(|s| s.matches(self.store.as_deref()))
            })
        {
            return;
        }
        let id = self.next_id();
        self.correlation.insert(id.clone(), request_id.to_owned());
        self.send(ClientFrame::ReceiptQuery {
            id,
            session: self.session.clone(),
            request_id: request_id.to_owned(),
        });
    }

    fn landed(&self, request_id: &str) -> bool {
        if self.requests.iter().any(|t| {
            t.request_id == request_id && matches!(t.source, Some(RequestSource::Uncertain { .. }))
        }) {
            return false;
        }
        let Some(entry) = self.receipts.get(request_id).and_then(|r| r.entry) else {
            return false;
        };
        self.transcript.has(entry) || self.transcript.newest().is_some_and(|n| entry <= n)
    }

    fn prune(&mut self) {
        let keep: Vec<bool> = self
            .requests
            .iter()
            .map(|t| {
                if !t.settled || matches!(t.source, Some(RequestSource::Uncertain { .. })) {
                    return true;
                }
                let names_entry = self
                    .receipts
                    .get(&t.request_id)
                    .is_some_and(|r| r.entry.is_some());
                t.request.shown() && !self.landed(&t.request_id) && names_entry
            })
            .collect();
        let mut index = 0;
        self.requests.retain(|_| {
            let keep = keep[index];
            index += 1;
            keep
        });
    }

    // ----- paging ---------------------------------------------------------------------

    fn read(
        &mut self,
        min: Option<u64>,
        max: u64,
        order: Order,
        cursor: Option<String>,
        state: Read,
    ) {
        let id = self.next_id();
        self.reads.insert(id.clone(), state);
        self.send(ClientFrame::ReadEntries {
            id,
            session: self.session.clone(),
            min,
            max: Some(max),
            order: Some(order),
            limit: Some(self.options.page_size),
            cursor,
        });
    }

    /// Fill the oldest gap with ascending pages until the range is exhausted.
    pub fn fill_gap(&mut self) {
        if !self.connected || self.reads.values().any(|r| matches!(r, Read::Gap { .. })) {
            return;
        }
        let Some(gap) = self.transcript.gaps().first().copied() else {
            return;
        };
        let (min, max) = (gap.read_min(), gap.read_max());
        let known = min.saturating_sub(1);
        self.read(
            Some(min),
            max,
            Order::Ascending,
            None,
            Read::Gap {
                gap,
                known,
                max,
                cursor: None,
                restarted: false,
            },
        );
    }

    /// Load one page of earlier entries, newest first.
    pub fn load_earlier(&mut self) {
        if !self.connected
            || self
                .reads
                .values()
                .any(|r| matches!(r, Read::Earlier { .. }))
        {
            return;
        }
        let Some(low) = self.transcript.earlier_bound() else {
            return;
        };
        let scan = self.earlier_scan.clone().filter(|s| s.low == Some(low));
        let max = scan.as_ref().map_or(low, |s| s.max);
        let cursor = scan.map(|s| s.cursor);
        self.read(
            None,
            max,
            Order::Descending,
            cursor.clone(),
            Read::Earlier { low, max, cursor },
        );
    }

    fn page_ok(&self, min: Option<u64>, max: u64, order: Order, items: &[Entry]) -> bool {
        let inside = items
            .iter()
            .all(|e| min.is_none_or(|m| e.id() >= m) && e.id() <= max);
        let ordered = items.windows(2).all(|pair| match order {
            Order::Ascending => pair[1].id() > pair[0].id(),
            Order::Descending => pair[1].id() < pair[0].id(),
        });
        inside && ordered && items.len() as u64 <= self.options.page_size
    }

    fn on_page(&mut self, read: Read, items: &[Entry], next: Option<String>) {
        let more = next.is_some() && !items.is_empty();
        match read {
            Read::Gap {
                gap, known, max, ..
            } => {
                if !self.page_ok(Some(known + 1), max, Order::Ascending, items) {
                    self.error = Some("invalid_page".to_owned());
                    return;
                }
                if self.transcript.cover_ascending(known, max, items, more) {
                    self.after_transcript_change();
                }
                if let (true, Some(last), Some(cursor)) = (more, items.last(), next) {
                    let known = last.id();
                    let state = Read::Gap {
                        gap,
                        known,
                        max,
                        cursor: Some(cursor.clone()),
                        restarted: false,
                    };
                    self.read(
                        Some(gap.read_min()),
                        max,
                        Order::Ascending,
                        Some(cursor),
                        state,
                    );
                }
            }
            Read::Earlier { low, max, .. } => {
                if !self.page_ok(None, max, Order::Descending, items) {
                    self.error = Some("invalid_page".to_owned());
                    return;
                }
                self.transcript.cover_descending(low, items, more);
                self.earlier_scan = match next {
                    Some(cursor) if more => Some(Scan {
                        max,
                        cursor,
                        low: self.transcript.earlier_bound(),
                    }),
                    _ => None,
                };
                self.after_transcript_change();
            }
        }
    }

    fn on_read_error(&mut self, read: Read, code: ErrorCode) {
        match read {
            Read::Gap {
                gap,
                cursor: Some(_),
                restarted: false,
                ..
            } if code == ErrorCode::Invalid => {
                // Cursor lifetime is not promised: restart once from our own bounds.
                let Some(current) = self
                    .transcript
                    .gaps()
                    .iter()
                    .find(|g| g.after < gap.up_to && gap.after < g.up_to)
                    .copied()
                else {
                    return;
                };
                let (min, max) = (current.read_min(), current.read_max());
                let state = Read::Gap {
                    gap: current,
                    known: min.saturating_sub(1),
                    max,
                    cursor: None,
                    restarted: true,
                };
                self.read(Some(min), max, Order::Ascending, None, state);
            }
            Read::Earlier {
                cursor: Some(_), ..
            } if code == ErrorCode::Invalid => {
                self.earlier_scan = None;
                self.load_earlier();
            }
            _ => self.error = Some(code.as_str()),
        }
    }

    fn after_transcript_change(&mut self) {
        self.prune();
        self.dirty = true;
    }

    // ----- frames in ------------------------------------------------------------------

    /// One host frame, in order.
    pub fn receive(&mut self, raw: &Value) {
        let frame = match HostFrame::parse(raw) {
            Ok(frame) => frame,
            Err(_) => {
                self.error = Some("invalid".to_owned());
                self.resync();
                return;
            }
        };
        let reply = frame.reply_to().map(str::to_owned);
        let request_id = reply.as_deref().and_then(|id| self.correlation.remove(id));
        let read = reply.as_deref().and_then(|id| self.reads.remove(id));
        match frame {
            HostFrame::Attached {
                id,
                attachment,
                session,
                store,
                ..
            } => self.on_attached(&id, attachment, &session, store),
            HostFrame::View {
                attachment,
                seq,
                ops,
                retired,
            } => self.on_view(&attachment, seq, &ops, retired),
            HostFrame::Entries { items, next, .. } => {
                if let Some(read) = read {
                    self.on_page(read, &items, next);
                }
            }
            HostFrame::Receipt(receipt) => self.on_receipt(receipt),
            HostFrame::Ended { attachment, reason } => self.on_ended(&attachment, reason),
            HostFrame::Detached { attachment, .. } => {
                if self.attachment.as_deref() == Some(attachment.as_str()) {
                    self.attachment = None;
                }
            }
            HostFrame::Error { id, code, .. } => self.on_error(&id, code, request_id, read),
        }
    }

    fn on_attached(&mut self, id: &str, attachment: String, session: &str, store: String) {
        if session != self.session || self.attach_id.as_deref() != Some(id) {
            return;
        }
        self.attach_id = None;
        self.attachment = Some(attachment);
        self.view = View::new();
        self.retired = false;
        if self.store.as_deref() != Some(store.as_str()) {
            if self.store.is_some() {
                // Entry ids of another store may name other content: drop the cache.
                self.transcript = Transcript::new();
                self.earlier_scan = None;
                self.correlation.clear();
                self.reads.clear();
                let tracked: Vec<String> =
                    self.requests.iter().map(|t| t.request_id.clone()).collect();
                self.receipts.retain(|id, _| tracked.contains(id));
            }
            self.store = Some(store);
            self.dirty = true;
        }
        for tracked in &mut self.requests {
            if let Some(source) = &mut tracked.source {
                source.bind(self.store.as_deref().unwrap_or_default());
            }
        }
        if !self.uncertain_requests().is_empty() {
            self.error = Some("store_changed_outcome_unknown".to_owned());
            self.notices.push(Notice::Refused {
                code: "store_changed_outcome_unknown".to_owned(),
            });
        }
        // All retries keep the original id, and only reach the proven original store.
        let pending: Vec<String> = self
            .requests
            .iter()
            .filter(|t| !t.settled)
            .map(|t| t.request_id.clone())
            .collect();
        for id in pending {
            self.resend(&id);
            self.query(&id);
        }
        self.dirty = true;
    }

    fn on_view(&mut self, attachment: &str, seq: u64, ops: &[Value], retired: Option<bool>) {
        if self.attachment.as_deref() != Some(attachment) {
            return;
        }
        if self.view.apply(seq, ops).is_err() {
            self.resync();
            return;
        }
        self.retired = retired == Some(true);
        self.cached = false;
        let entries = self.view.entries();
        if self.transcript.merge(&entries, self.view.before()) {
            self.after_transcript_change();
        }
    }

    fn on_receipt(&mut self, receipt: Receipt) {
        if receipt.session != self.session
            || self.store.is_none()
            || self.requests.iter().any(|t| {
                t.request_id == receipt.request_id
                    && !t
                        .source
                        .as_ref()
                        .is_some_and(|s| s.matches(self.store.as_deref()))
            })
        {
            return;
        }
        if let Some(old) = self.receipts.get(&receipt.request_id)
            && (old.kind != receipt.kind || !old.status.may_become(receipt.status))
        {
            self.error = Some("invalid".to_owned());
            return;
        }
        let mut clean = receipt;
        clean.id = None;
        if terminal(clean.status)
            && let Some(tracked) = self
                .requests
                .iter_mut()
                .find(|t| t.request_id == clean.request_id)
        {
            tracked.settled = true;
        }
        self.receipts.insert(clean.request_id.clone(), clean);
        self.after_transcript_change();
    }

    fn on_ended(&mut self, attachment: &str, reason: EndedReason) {
        if self.attachment.as_deref() != Some(attachment) {
            return;
        }
        self.attachment = None;
        match reason {
            EndedReason::Denied => self.purge_all(),
            EndedReason::Failed if self.connected => {
                self.attach(); // Receipt queries, like retries, wait for on_attached's store fence.
            }
            _ => {}
        }
    }

    fn on_error(
        &mut self,
        id: &str,
        code: ErrorCode,
        request_id: Option<String>,
        read: Option<Read>,
    ) {
        let attach = self.attach_id.as_deref() == Some(id);
        if attach {
            self.attach_id = None;
        }
        let ours = attach || request_id.is_some() || read.is_some();
        if !ours {
            return;
        }
        self.error = Some(code.as_str());
        if ours && purging(code) {
            self.purge_all();
            return;
        }
        if let Some(read) = read {
            self.on_read_error(read, code);
            return;
        }
        let Some(request_id) = request_id else { return };
        if self.retry_as_follow_up(&request_id, code) {
            return;
        }
        if definitive(code) {
            self.notices.push(Notice::Refused {
                code: code.as_str(),
            });
            let before = self.requests.len();
            self.requests.retain(|t| t.request_id != request_id);
            if self.requests.len() != before {
                self.dirty = true;
            }
        }
    }

    /// A `busy` prompt or an `idle` steer left no record: send it again as a follow-up under
    /// the same request id (allowed by the contract).
    fn retry_as_follow_up(&mut self, request_id: &str, code: ErrorCode) -> bool {
        let Some(tracked) = self
            .requests
            .iter_mut()
            .find(|t| t.request_id == request_id)
        else {
            return false;
        };
        let Request::Submit { input } = &mut tracked.request else {
            return false;
        };
        let retry = matches!(
            (input.kind, code),
            (InputKind::Prompt, ErrorCode::Busy) | (InputKind::Steer, ErrorCode::Idle)
        );
        if retry {
            input.kind = InputKind::FollowUp;
            self.error = None;
            self.dirty = true;
            let id = request_id.to_owned();
            self.resend(&id);
        }
        retry
    }

    /// Forget everything held for this Session and ask the runtime to delete its cache.
    pub fn purge_all(&mut self) {
        if self.requests.iter().any(|t| !t.settled) {
            self.error = Some("access_lost_outcome_unknown".to_owned());
            self.notices.push(Notice::Refused {
                code: "access_lost_outcome_unknown".to_owned(),
            });
        }
        self.attachment = None;
        self.attach_id = None;
        self.outgoing.clear();
        self.correlation.clear();
        self.reads.clear();
        self.view = View::new();
        self.transcript = Transcript::new();
        self.requests.clear();
        self.receipts.clear();
        self.store = None;
        self.earlier_scan = None;
        self.cached = false;
        self.dirty = false;
        self.purge = true;
        self.notices.push(Notice::Purged);
    }

    /// Receipts of a given kind still unsettled (for example queued inputs this client sent).
    pub fn unsettled(&self, kind: ReceiptKind) -> usize {
        self.receipts
            .values()
            .filter(|r| r.kind == kind && !r.status.is_final())
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn attached(id: &str, store: &str) -> Value {
        json!({"type": "attached", "id": id, "v": 0, "attachment": "att_1", "session": "ses", "store": store,
            "window": {"min": 50, "max": 100}, "upstream": {"package": "@earendil-works/pi-durable", "version": "1.5.2"},
            "max_frame_bytes": 8388608})
    }

    #[test]
    fn instant_send_offline_then_resend_on_connect() {
        let mut client = SessionClient::new("ses", Options::default());
        let id = client.submit(InputKind::Prompt, "hello");
        assert!(client.take_outgoing().is_empty());
        assert!(matches!(client.items().last(), Some(Item::Pending { .. })));
        client.connect();
        let out = client.take_outgoing();
        assert!(matches!(out[0], ClientFrame::Attach { .. }));
        assert_eq!(out.len(), 1);
        client.receive(&attached(out[0].id(), "st_a"));
        let resumed = client.take_outgoing();
        assert_eq!(resumed[0].request_id(), Some(id.as_str()));
    }

    #[test]
    fn busy_prompt_is_retried_as_follow_up_with_the_same_request_id() {
        let mut client = SessionClient::new("ses", Options::default());
        client.connect();
        let attach = client.take_outgoing();
        client.receive(&attached(attach[0].id(), "st_a"));
        let id = client.submit(InputKind::Prompt, "more");
        let out = client.take_outgoing();
        let frame_id = out[0].id().to_owned();
        client.receive(&json!({"type": "error", "id": frame_id, "code": "busy"}));
        let retry = client.take_outgoing();
        match &retry[..] {
            [
                ClientFrame::Submit {
                    request_id, input, ..
                },
            ] => {
                assert_eq!(request_id, &id);
                assert_eq!(input.kind, InputKind::FollowUp);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn store_change_drops_the_cached_transcript() {
        let mut client = SessionClient::new("ses", Options::default());
        client.connect();
        let attach = client.take_outgoing()[0].id().to_owned();
        client.receive(&attached(&attach, "st_a"));
        client.receive(&json!({"type": "view", "attachment": "att_1", "seq": 1, "ops": [["r", {"conversation": {"id": 1},
            "entries": [{"id": 3, "conversationId": 1, "kind": "pi.user"}], "docs": {}}]]}));
        assert_eq!(client.transcript().entries().len(), 1);
        let record = client.record();
        let mut next = SessionClient::new("ses", Options::default());
        next.restore(record);
        assert!(next.cached());
        next.connect();
        let out = next.take_outgoing();
        let ClientFrame::Attach { id, window, .. } = &out[0] else {
            panic!("attach first")
        };
        assert_eq!(window.as_ref().and_then(|w| w.from), Some(4));
        next.receive(&attached(id, "st_b"));
        assert!(next.transcript().entries().is_empty());
    }

    #[test]
    fn denied_purges() {
        let mut client = SessionClient::new("ses", Options::default());
        client.connect();
        let attach = client.take_outgoing()[0].id().to_owned();
        client.receive(&json!({"type": "error", "id": attach, "code": "unknown_session"}));
        assert!(client.take_purge());
    }
}
