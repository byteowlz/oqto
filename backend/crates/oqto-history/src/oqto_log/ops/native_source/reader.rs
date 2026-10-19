//! A process-wide SQLite worker/connection bound, independent of shard count
//! and concurrent validators. Main is empty memory; at most one read-only shard
//! is attached per borrowed connection. Ownership answers are never cached.
use super::*;
use sqlx::pool::PoolConnection;
use std::{sync::LazyLock, time::Duration};

pub(super) const READ_CONNECTIONS: u32 = 2;
static READERS: LazyLock<sqlx::SqlitePool> = LazyLock::new(|| {
    SqlitePoolOptions::new()
        .max_connections(READ_CONNECTIONS)
        .min_connections(0)
        .max_lifetime(None)
        .idle_timeout(None)
        .acquire_timeout(Duration::from_secs(2))
        .connect_lazy_with(
            SqliteConnectOptions::new()
                .filename(":memory:")
                .in_memory(true)
                .statement_cache_capacity(0)
                .busy_timeout(Duration::from_millis(100)),
        )
});

pub(in crate::oqto_log::ops) struct ReadShard(PoolConnection<sqlx::Sqlite>);
impl ReadShard {
    pub(in crate::oqto_log::ops) async fn open(path: &Path) -> Result<Self> {
        let mut connection = READERS.acquire().await?;
        // A cancelled reader can return its connection with an attachment still
        // present. Never reuse it as authority: detach BEFORE opening the exact
        // requested path. Failed cleanup refuses the operation, not a fallback.
        let attached: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pragma_database_list WHERE name='native_identity')",
        )
        .fetch_one(&mut *connection)
        .await?;
        if attached {
            sqlx::query("DETACH DATABASE native_identity")
                .execute(&mut *connection)
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
            .execute(&mut *connection)
            .await?;
        Ok(Self(connection))
    }
    pub(in crate::oqto_log::ops) fn connection(&mut self) -> &mut sqlx::SqliteConnection {
        &mut self.0
    }
    pub(in crate::oqto_log::ops) async fn finish(mut self) -> Result<()> {
        sqlx::query("DETACH DATABASE native_identity")
            .execute(&mut *self.0)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn pool_size() -> u32 {
    READERS.size()
}
