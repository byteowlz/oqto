use crate::{PlacementId, PlacementRecord};
use anyhow::{Context, Result};
use async_trait::async_trait;
use oqto_runner::transport::RunnerEndpointConfig;
use std::collections::BTreeMap;
use std::path::PathBuf;
use tokio::sync::RwLock;

#[async_trait]
pub trait PlacementStore: Send + Sync {
    async fn put(&self, placement: PlacementRecord) -> Result<()>;
    async fn get(&self, id: &PlacementId) -> Result<Option<PlacementRecord>>;
    async fn resolve_workspace(&self, workspace_id: &str) -> Result<Option<RunnerEndpointConfig>>;
    async fn find_workspace(&self, workspace_id: &str) -> Result<Option<PlacementRecord>>;
    async fn list(&self) -> Result<Vec<PlacementRecord>>;
    async fn remove(&self, id: &PlacementId) -> Result<()>;
}

pub struct JsonPlacementStore {
    path: PathBuf,
    records: RwLock<BTreeMap<String, PlacementRecord>>,
}

impl JsonPlacementStore {
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let records = match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("parsing placement store {}", path.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("reading placement store {}", path.display()));
            }
        };
        Ok(Self {
            path,
            records: RwLock::new(records),
        })
    }

    async fn persist(&self, records: &BTreeMap<String, PlacementRecord>) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await.with_context(|| {
                format!("creating placement store directory {}", parent.display())
            })?;
        }
        let bytes = serde_json::to_vec_pretty(records).context("serializing placement store")?;
        let temporary = self.path.with_extension("json.tmp");
        tokio::fs::write(&temporary, bytes)
            .await
            .with_context(|| format!("writing placement store temp {}", temporary.display()))?;
        tokio::fs::rename(&temporary, &self.path)
            .await
            .with_context(|| format!("activating placement store {}", self.path.display()))?;
        Ok(())
    }
}

#[async_trait]
impl PlacementStore for JsonPlacementStore {
    async fn put(&self, placement: PlacementRecord) -> Result<()> {
        let mut records = self.records.write().await;
        // Workspace routing is one-to-one. Replacements must evict stale
        // lifecycle records so endpoint resolution cannot select an old runner.
        records.retain(|_, record| record.workspace_id != placement.workspace_id);
        records.insert(placement.id.0.clone(), placement);
        self.persist(&records).await
    }

    async fn get(&self, id: &PlacementId) -> Result<Option<PlacementRecord>> {
        Ok(self.records.read().await.get(&id.0).cloned())
    }

    async fn resolve_workspace(&self, workspace_id: &str) -> Result<Option<RunnerEndpointConfig>> {
        Ok(self
            .records
            .read()
            .await
            .values()
            .find(|record| record.workspace_id == workspace_id)
            .map(|record| record.runner_endpoint.clone()))
    }

    async fn find_workspace(&self, workspace_id: &str) -> Result<Option<PlacementRecord>> {
        Ok(self
            .records
            .read()
            .await
            .values()
            .find(|record| record.workspace_id == workspace_id)
            .cloned())
    }

    async fn list(&self) -> Result<Vec<PlacementRecord>> {
        Ok(self.records.read().await.values().cloned().collect())
    }

    async fn remove(&self, id: &PlacementId) -> Result<()> {
        let mut records = self.records.write().await;
        records.remove(&id.0);
        self.persist(&records).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PlacementKind, PlacementRecord};

    #[tokio::test]
    async fn store_round_trips_workspace_endpoint() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("placements.json");
        let store = JsonPlacementStore::open(&path).await?;
        let endpoint = RunnerEndpointConfig::Unix {
            path: PathBuf::from("/run/oqto/ws/runner.sock"),
        };
        store
            .put(PlacementRecord {
                id: PlacementId("placement-1".to_string()),
                workspace_id: "workspace-1".to_string(),
                account_id: "account-1".to_string(),
                kind: PlacementKind::LocalProcess,
                runner_endpoint: endpoint.clone(),
                runtime_name: "local".to_string(),
                spec: None,
            })
            .await?;
        assert_eq!(
            store.resolve_workspace("workspace-1").await?,
            Some(endpoint)
        );
        let replacement_endpoint = RunnerEndpointConfig::Unix {
            path: PathBuf::from("/run/oqto/ws/replacement.sock"),
        };
        store
            .put(PlacementRecord {
                id: PlacementId("placement-2".to_string()),
                workspace_id: "workspace-1".to_string(),
                account_id: "account-1".to_string(),
                kind: PlacementKind::LocalProcess,
                runner_endpoint: replacement_endpoint.clone(),
                runtime_name: "replacement".to_string(),
                spec: None,
            })
            .await?;
        assert!(
            store
                .get(&PlacementId("placement-1".to_string()))
                .await?
                .is_none()
        );
        assert_eq!(
            store.resolve_workspace("workspace-1").await?,
            Some(replacement_endpoint)
        );

        let reopened = JsonPlacementStore::open(path).await?;
        assert!(
            reopened
                .get(&PlacementId("placement-2".to_string()))
                .await?
                .is_some()
        );
        Ok(())
    }
}
