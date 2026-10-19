//! Two process-lifetime SQLite readers, shared across shards/homes/validators.
//! No caller owns establishment or connection destruction/replacement. SQLx pool
//! slots are NOT physical-worker lifetime bounds when acquire is cancelled.
use super::*;
use sqlx::{ConnectOptions, Connection, SqliteConnection};
use std::{sync::Arc, sync::LazyLock, time::Duration};
use tokio::sync::{Mutex, OwnedMutexGuard, mpsc};

pub(super) const READ_CONNECTIONS: u32 = 2;
type Slot = std::result::Result<Arc<Mutex<SqliteConnection>>, Arc<str>>;
struct Readers {
    available: mpsc::UnboundedSender<Slot>,
    receiver: Mutex<mpsc::UnboundedReceiver<Slot>>,
}
static READERS: LazyLock<Readers> = LazyLock::new(|| {
    let (available, receiver) = mpsc::unbounded_channel();
    // Exactly two connect attempts for the lifetime of this process. A small
    // initializer thread per slot, with its own runtime, cannot be aborted by a
    // requesting future OR its runtime shutting down. It exits after handing off
    // the connection; only SQLx's two dedicated SQLite workers remain resident.
    for slot in 0..READ_CONNECTIONS {
        let sender = available.clone();
        let started = std::thread::Builder::new()
            .name(format!("oqto-reader-init-{slot}"))
            .spawn(move || {
                let result = initialize().map(|connection| Arc::new(Mutex::new(connection)));
                let _ = sender.send(result.map_err(|error| Arc::from(error.to_string())));
            });
        if let Err(error) = started {
            let _ = available.send(Err(Arc::from(error.to_string())));
        }
    }
    Readers {
        available,
        receiver: Mutex::new(receiver),
    }
});

fn initialize() -> Result<SqliteConnection> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let connection = SqliteConnectOptions::new()
                .filename(":memory:")
                .in_memory(true)
                .statement_cache_capacity(0)
                .busy_timeout(Duration::from_millis(100))
                .connect()
                .await?;
            #[cfg(test)]
            let mut connection = connection;
            #[cfg(test)]
            lifecycle_tests::established(&mut connection).await?;
            #[cfg(test)]
            INITIALIZED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(connection)
        })
}

// The static queue is never closed/dropped. Returning an Arc synchronously on
// every exit (including cancellation between dequeue and locking) keeps the
// connection alive independently of any caller. Error slots also circulate:
// failed initialization/worker death refuses, never spawns an unsafe replacement.
struct ReturnSlot(Slot);
impl Drop for ReturnSlot {
    fn drop(&mut self) {
        let _ = READERS.available.send(self.0.clone());
    }
}

pub(in crate::oqto_log::ops) struct ReadShard {
    connection: OwnedMutexGuard<SqliteConnection>,
    _slot: ReturnSlot,
}
impl ReadShard {
    pub(in crate::oqto_log::ops) async fn open(path: &Path) -> Result<Self> {
        let slot = ReturnSlot(
            tokio::time::timeout(Duration::from_secs(2), async {
                READERS.receiver.lock().await.recv().await
            })
            .await?
            .context("Native reader queue closed")?,
        );
        let connection = match &slot.0 {
            Ok(connection) => connection.clone().lock_owned().await,
            Err(error) => anyhow::bail!("Native reader initialization failed: {error}"),
        };
        // Own the return path BEFORE the first await touching this connection.
        let mut shard = Self {
            connection,
            _slot: slot,
        };
        #[cfg(test)]
        lifecycle_tests::idle_acquire(shard.connection()).await?;
        shard.connection.ping().await?;
        // Cancelled operations may leave queued SQL/attachments. SQLx serializes
        // commands on this SAME worker; cleanup must complete before new ATTACH.
        let attached: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pragma_database_list WHERE name='native_identity')",
        )
        .fetch_one(shard.connection())
        .await?;
        if attached {
            sqlx::query("DETACH DATABASE native_identity")
                .execute(shard.connection())
                .await?;
        }
        // Bind a SQLite URI, encoding ALL special bytes (including literal %).
        // mode=ro is mandatory: no missing database can be created on a read.
        let mut uri = String::from("file:");
        use std::fmt::Write;
        for byte in path.as_os_str().as_encoded_bytes() {
            if byte.is_ascii_alphanumeric() || b"/._-".contains(byte) {
                uri.push(*byte as char);
            } else {
                write!(&mut uri, "%{byte:02X}")?;
            }
        }
        uri.push_str("?mode=ro&cache=private");
        sqlx::query("ATTACH DATABASE ? AS native_identity")
            .bind(uri)
            .execute(shard.connection())
            .await?;
        Ok(shard)
    }
    pub(in crate::oqto_log::ops) fn connection(&mut self) -> &mut SqliteConnection {
        &mut self.connection
    }
    pub(in crate::oqto_log::ops) async fn finish(mut self) -> Result<()> {
        sqlx::query("DETACH DATABASE native_identity")
            .execute(self.connection())
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
static INITIALIZED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
// Keep the earlier resource assertions; cancellation proof additionally observes
// ConnectionState-owned captures and real worker thread identities, not this count.
#[cfg(test)]
pub(super) fn pool_size() -> u32 {
    INITIALIZED.load(std::sync::atomic::Ordering::SeqCst)
}
