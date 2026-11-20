//! Private Durable state bootstrap and exclusive store ownership, not a brain launcher.
//! OS ownership protects against other Principals; sandbox exclusion of control state
//! and authenticated AgentHome derivation must still be established by supervision.
use crate::durable_catalog::{AgentHome, CatalogSession, DurableCatalog, StoreRecord};
use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteSynchronous};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

mod acl;
mod files;
mod filesystem;

pub struct DurableState {
    files: Arc<files::Layout>,
    catalog: DurableCatalog,
}

/// The lock is outside data_directory and is retained until this value is dropped.
/// This record is not a readiness receipt, a sandbox or a resource grant.
/// Acquiring it does not prove that a previous brain has stopped; supervision must
/// independently confirm that before launching any replacement.
pub struct StoreLease {
    record: StoreRecord,
    directory: Arc<std::fs::File>,
    path: PathBuf,
    layout: Arc<files::Layout>,
    _lock: std::fs::File,
}
impl DurableState {
    /// Admit a dedicated root, without changing permissions on existing objects.
    /// Only the leaf may be created; its parent must already exist and be trusted.
    pub async fn open(root: &Path) -> Result<Self> {
        let root = root.to_owned();
        let prepared = tokio::task::spawn_blocking(move || files::prepare(&root)).await??;
        let options = SqliteConnectOptions::new()
            .filename(prepared.layout.database())
            .create_if_missing(false)
            .foreign_keys(true)
            .synchronous(SqliteSynchronous::Full)
            .busy_timeout(std::time::Duration::from_secs(3));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        let catalog = DurableCatalog::open(pool).await?;
        let files = prepared.layout.clone();
        tokio::task::spawn_blocking(move || files.check()).await??;
        Ok(Self {
            files: prepared.layout,
            catalog,
        })
        // prepared's initialization lock closes here; its inode is never unlinked.
    }

    pub async fn lease_store(&self, home: &AgentHome) -> Result<StoreLease> {
        self.check().await?;
        let record = self.catalog.reserve_store(home).await?;
        self.check().await?;
        let layout = self.files.clone();
        tokio::task::spawn_blocking(move || {
            let (path, directory, lock) = layout.lease(&record.store_id)?;
            Ok(StoreLease {
                record,
                directory: Arc::new(directory),
                path,
                layout,
                _lock: lock,
            })
        })
        .await?
    }

    pub async fn bind_session(
        &self,
        home: &AgentHome,
        public: &str,
        native: &str,
        origin: &str,
    ) -> Result<CatalogSession> {
        self.check().await?;
        let binding = self
            .catalog
            .bind_session(home, public, native, origin)
            .await?;
        self.check().await?;
        Ok(binding)
    }

    pub async fn resolve(&self, home: &AgentHome, public: &str) -> Result<Option<CatalogSession>> {
        self.check().await?;
        let binding = self.catalog.resolve(home, public).await?;
        self.check().await?;
        Ok(binding)
    }

    async fn check(&self) -> Result<()> {
        let files = self.files.clone();
        tokio::task::spawn_blocking(move || files.check()).await?
    }

    pub async fn close(&self) {
        self.catalog.close().await;
    }
}

impl StoreLease {
    pub fn record(&self) -> &StoreRecord {
        &self.record
    }
    pub fn data_directory(&self) -> &Path {
        &self.path
    }

    /// Check again before launch/delivery; no process is started by this API.
    pub async fn revalidate(&self) -> Result<()> {
        let layout = self.layout.clone();
        let directory = self.directory.clone();
        let path = self.path.clone();
        let id = self.record.store_id.clone();
        let lock = self
            ._lock
            .try_clone()
            .context("retain Durable store lock")?;
        tokio::task::spawn_blocking(move || {
            layout.check()?;
            files::same_directory(&path, &directory)?;
            files::same_lock(&layout.lock_path(&id), &lock)
        })
        .await?
    }
}

#[cfg(test)]
mod tests;
