//! The async runtime around [`SessionClient`]: fast start from the cache, connect, move frames,
//! save the cache on change, reconnect with backoff. A UI reads state through
//! [`SessionHandle::read`], changes it through [`SessionHandle::update`] and redraws when
//! [`SessionHandle::changed`] fires.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use anyhow::Result;
use oqto_session_view::CacheKey;
use oqto_session_view::frames::ClientFrame;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use crate::cache::FileCache;
use crate::client::{Notice, Options, SessionClient};
use crate::transport::{self, Incoming, Secret};

/// The state of the link to the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// Trying to connect.
    Connecting,
    /// Connected and authenticated.
    Online,
    /// Not connected; the reason in plain words.
    Offline(String),
}

/// Everything a UI reads.
#[derive(Debug)]
pub struct Shared {
    /// The Session client.
    pub client: SessionClient,
    /// The link.
    pub link: Link,
    /// Notices not yet shown.
    pub notices: Vec<Notice>,
    /// The cache could not be read or written (shown, never fatal).
    pub cache_error: Option<String>,
}

/// How to reach one Session.
#[derive(Debug)]
pub struct SessionConfig {
    /// WebSocket URL of the host's `/session` path.
    pub url: String,
    /// Bearer secret.
    pub secret: Secret,
    /// Session id.
    pub session: String,
    /// Window and page size.
    pub options: Options,
    /// Encrypted cache, if any.
    pub cache: Option<Arc<FileCache>>,
    /// Entries kept in the cache at most.
    pub max_cached_entries: usize,
    /// Connect timeout.
    pub connect_timeout: Duration,
}

/// A running Session client.
#[derive(Clone)]
pub struct SessionHandle {
    shared: Arc<Mutex<Shared>>,
    frames: mpsc::UnboundedSender<Vec<ClientFrame>>,
    changes: watch::Sender<u64>,
    cache: Option<Arc<FileCache>>,
    key: CacheKey,
    max_cached_entries: usize,
}

fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl SessionHandle {
    /// Load the cache (fast start, before any network) and spawn the connection task.
    pub fn start(config: SessionConfig) -> (Self, JoinHandle<()>) {
        let key = CacheKey::new(config.url.clone(), config.session.clone());
        let mut client = SessionClient::new(config.session.clone(), config.options.clone());
        let mut cache_error = None;
        if let Some(cache) = &config.cache {
            match cache.load(&key) {
                Ok(Some(record)) => client.restore(record),
                Ok(None) => {}
                Err(error) => cache_error = Some(format!("{error:#}")),
            }
        }
        let shared = Arc::new(Mutex::new(Shared {
            client,
            link: Link::Connecting,
            notices: Vec::new(),
            cache_error,
        }));
        let (frames, outgoing) = mpsc::unbounded_channel();
        let (changes, _) = watch::channel(0);
        let handle = Self {
            shared,
            frames,
            changes,
            cache: config.cache.clone(),
            key,
            max_cached_entries: config.max_cached_entries,
        };
        let task = tokio::spawn(run(handle.clone(), config, outgoing));
        (handle, task)
    }

    /// Read the state.
    pub fn read<R>(&self, f: impl FnOnce(&Shared) -> R) -> R {
        f(&lock(&self.shared))
    }

    /// Change the client (submit, abort, page); frames go out at once, the cache is saved.
    pub fn update<R>(&self, f: impl FnOnce(&mut SessionClient) -> R) -> R {
        let (result, frames) = {
            let mut guard = lock(&self.shared);
            let result = f(&mut guard.client);
            (result, self.settle(&mut guard))
        };
        if !frames.is_empty() {
            let _ = self.frames.send(frames);
        }
        self.bump();
        result
    }

    /// Take notices for the status line.
    pub fn take_notices(&self) -> Vec<Notice> {
        std::mem::take(&mut lock(&self.shared).notices)
    }

    /// A receiver that fires whenever the state changed.
    pub fn changed(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    fn bump(&self) {
        self.changes.send_modify(|n| *n = n.wrapping_add(1));
    }

    /// Outgoing frames now; save or purge the cache when needed.
    fn settle(&self, shared: &mut Shared) -> Vec<ClientFrame> {
        let notices = shared.client.take_notices();
        shared.notices.extend(notices);
        if let Some(cache) = &self.cache {
            if shared.client.take_purge()
                && let Err(error) = cache.remove(&self.key)
            {
                shared.cache_error = Some(format!("{error:#}"));
            }
            if shared.client.take_dirty() {
                let mut record = shared.client.record();
                record.truncate_oldest(self.max_cached_entries);
                if let Err(error) = cache.save(&self.key, &record) {
                    shared.cache_error = Some(format!("{error:#}"));
                }
            }
        }
        shared.client.take_outgoing()
    }

    fn set_link(&self, link: Link) {
        lock(&self.shared).link = link;
        self.bump();
    }
}

async fn send_all(connection: &mut transport::Connection, frames: Vec<ClientFrame>) -> Result<()> {
    for frame in frames {
        connection.send(&frame.to_value()).await?;
    }
    Ok(())
}

async fn run(
    handle: SessionHandle,
    config: SessionConfig,
    mut outgoing: mpsc::UnboundedReceiver<Vec<ClientFrame>>,
) {
    let mut failures: u32 = 0;
    loop {
        handle.set_link(Link::Connecting);
        let mut connection =
            match transport::connect(&config.url, &config.secret, config.connect_timeout).await {
                Ok(connection) => connection,
                Err(error) => {
                    handle.set_link(Link::Offline(format!("{error:#}")));
                    failures = failures.saturating_add(1);
                    let wait = Duration::from_millis(
                        250u64.saturating_mul(1 << failures.min(6)).min(10_000),
                    );
                    tokio::time::sleep(wait).await;
                    continue;
                }
            };
        failures = 0;
        while outgoing.try_recv().is_ok() {}
        let first = {
            let mut guard = lock(&handle.shared);
            guard.client.connect();
            guard.link = Link::Online;
            handle.settle(&mut guard)
        };
        handle.bump();
        let reason = match send_all(&mut connection, first).await {
            Ok(()) => pump(&handle, &mut connection, &mut outgoing).await,
            Err(error) => format!("{error:#}"),
        };
        {
            let mut guard = lock(&handle.shared);
            guard.client.disconnect();
            guard.link = Link::Offline(reason);
        }
        handle.bump();
        connection.close().await;
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn pump(
    handle: &SessionHandle,
    connection: &mut transport::Connection,
    outgoing: &mut mpsc::UnboundedReceiver<Vec<ClientFrame>>,
) -> String {
    loop {
        tokio::select! {
            incoming = connection.next() => match incoming {
                Incoming::Frame(value) => {
                    let frames = {
                        let mut guard = lock(&handle.shared);
                        guard.client.receive(&value);
                        handle.settle(&mut guard)
                    };
                    handle.bump();
                    if let Err(error) = send_all(connection, frames).await {
                        return format!("{error:#}");
                    }
                }
                Incoming::Closed(reason) => return reason,
            },
            frames = outgoing.recv() => match frames {
                Some(frames) => {
                    if let Err(error) = send_all(connection, frames).await {
                        return format!("{error:#}");
                    }
                }
                None => return "client stopped".to_owned(),
            },
        }
    }
}
