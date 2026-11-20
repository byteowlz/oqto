//! Runner-owned Durable identity, independent of runtime artifacts and transcripts.
//!
//! This source library adds no listener or authority. Bootstrap must supply a dedicated,
//! private SQLite pool admitted as the owning OS Principal; brains must never see it.
//! Agent-home scope is trusted Runner context, not a caller claim or a bearer grant.
use anyhow::{Context, Result, ensure};
use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations_durable_catalog");

/// The agent's owning Workspace, not the work directory of its current task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentHome {
    pub workspace_id: String,
    pub agent_id: String,
}
impl AgentHome {
    fn validate(&self) -> Result<()> {
        identity(&self.workspace_id)?;
        identity(&self.agent_id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreRecord {
    pub store_id: String,
    pub home: AgentHome,
}

/// Exact storage/native mapping. Possessing this record confers no resource access.
#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow)]
pub struct CatalogSession {
    pub public_session_id: String,
    pub store_id: String,
    pub native_conversation_id: String,
    /// Initial project association, not live cwd or complete access provenance.
    pub origin_work_directory_id: String,
}
impl CatalogSession {
    fn validate(&self) -> Result<()> {
        identity(&self.public_session_id)?;
        store_identity(&self.store_id)?;
        identity(&self.native_conversation_id)?;
        identity(&self.origin_work_directory_id)
    }
}

#[derive(Clone)]
pub struct DurableCatalog {
    pool: SqlitePool,
}
impl DurableCatalog {
    /// Migrate only a dedicated catalog pool already admitted by private bootstrap.
    /// Do not pass an oqto-log, pairing, permission or upstream Session database.
    pub async fn open(pool: SqlitePool) -> Result<Self> {
        admit_schema(&pool).await?;
        MIGRATOR
            .run(&pool)
            .await
            .context("migrate Durable catalog")?;
        Ok(Self { pool })
    }

    /// Idempotent across tasks, catalog reopen and runtime changes. No brain is started.
    pub async fn reserve_store(&self, home: &AgentHome) -> Result<StoreRecord> {
        home.validate()?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let store_id = ensure_store(&mut tx, home).await?;
        tx.commit().await.context("commit Durable store identity")?;
        Ok(StoreRecord {
            store_id,
            home: home.clone(),
        })
    }

    /// Register an already-created native conversation; never execute or import it.
    /// Exact retries return the same binding; any identity change is refused.
    pub async fn bind_session(
        &self,
        home: &AgentHome,
        public_session_id: &str,
        native_conversation_id: &str,
        origin_work_directory_id: &str,
    ) -> Result<CatalogSession> {
        home.validate()?;
        identity(public_session_id)?;
        identity(native_conversation_id)?;
        identity(origin_work_directory_id)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let binding = CatalogSession {
            public_session_id: public_session_id.into(),
            store_id: ensure_store(&mut tx, home).await?,
            native_conversation_id: native_conversation_id.into(),
            origin_work_directory_id: origin_work_directory_id.into(),
        };
        sqlx::query("INSERT INTO durable_sessions(public_session_id,store_id,native_conversation_id,origin_work_directory_id) VALUES(?,?,?,?) ON CONFLICT(public_session_id) DO NOTHING")
            .bind(&binding.public_session_id)
            .bind(&binding.store_id)
            .bind(&binding.native_conversation_id)
            .bind(&binding.origin_work_directory_id)
            .execute(&mut *tx)
            .await
            .context("register unique Durable conversation")?;
        let existing = sqlx::query_as::<_, CatalogSession>(
            "SELECT public_session_id,store_id,native_conversation_id,origin_work_directory_id FROM durable_sessions WHERE public_session_id=? AND store_id=?",
        )
        .bind(public_session_id)
        .bind(&binding.store_id)
        .fetch_optional(&mut *tx)
        .await?;
        ensure!(
            existing.as_ref() == Some(&binding),
            "Durable Session identity conflict"
        );
        tx.commit()
            .await
            .context("commit Durable Session binding")?;
        Ok(binding)
    }

    /// Unknown and foreign-scope ids both return None. Never infer an index or path.
    pub async fn resolve(
        &self,
        home: &AgentHome,
        public_session_id: &str,
    ) -> Result<Option<CatalogSession>> {
        home.validate()?;
        identity(public_session_id)?;
        let row = sqlx::query_as::<_, CatalogSession>(
            "SELECT s.public_session_id,s.store_id,s.native_conversation_id,s.origin_work_directory_id FROM durable_sessions s JOIN durable_stores d ON d.store_id=s.store_id WHERE d.workspace_id=? AND d.agent_id=? AND s.public_session_id=?",
        )
        .bind(&home.workspace_id)
        .bind(&home.agent_id)
        .bind(public_session_id)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(binding) = &row {
            binding.validate()?;
        }
        Ok(row)
    }

    /// Close the shared backing pool during orderly owner shutdown.
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

fn identity(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 256
            && value.trim() == value
            && !value.chars().any(char::is_control)
            && !value.starts_with("pending-")
            && !value.starts_with("tmp:"),
        "Stable Durable identity required"
    );
    Ok(())
}

fn store_identity(value: &str) -> Result<()> {
    ensure!(
        Uuid::parse_str(value)?.to_string() == value,
        "Invalid Durable store identity"
    );
    Ok(())
}

// This distinguishes databases, not OS ownership or access authority. A known marker
// with no tables can be the result of a crash before the first migration completed.
async fn admit_schema(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin_with("BEGIN EXCLUSIVE").await?;
    let application: i64 = sqlx::query_scalar("PRAGMA application_id")
        .fetch_one(&mut *tx)
        .await?;
    if application == 0 {
        let tables: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*'")
                .fetch_one(&mut *tx)
                .await?;
        ensure!(
            tables == 0,
            "Refusing nonempty unrecognized Durable catalog"
        );
        sqlx::query("PRAGMA application_id=0x4f514443")
            .execute(&mut *tx)
            .await?;
    } else {
        ensure!(
            application == 0x4f514443,
            "Foreign Durable catalog application identity"
        );
    }
    tx.commit().await.context("admit Durable catalog schema")
}

async fn ensure_store(tx: &mut Transaction<'_, Sqlite>, home: &AgentHome) -> Result<String> {
    sqlx::query("INSERT INTO durable_stores(store_id,workspace_id,agent_id) VALUES(?,?,?) ON CONFLICT(workspace_id,agent_id) DO NOTHING")
        .bind(Uuid::new_v4().to_string())
        .bind(&home.workspace_id)
        .bind(&home.agent_id)
        .execute(&mut **tx)
        .await?;
    let id: String = sqlx::query_scalar(
        "SELECT store_id FROM durable_stores WHERE workspace_id=? AND agent_id=?",
    )
    .bind(&home.workspace_id)
    .bind(&home.agent_id)
    .fetch_one(&mut **tx)
    .await?;
    store_identity(&id)?;
    Ok(id)
}

#[cfg(test)]
mod tests;
