//! Bounded idle pool retention. Eviction drops only the cache's clone;
//! active callers retain their pool and are never explicitly closed here.
use sqlx::SqlitePool;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

// Leave descriptor headroom for SQLite WAL files, readers, indexes and RPCs,
// including on workstation shells with small RLIMIT_NOFILE budgets.
const RETAINED_POOLS: usize = 4;

#[derive(Default)]
pub(crate) struct PoolCache {
    entries: VecDeque<(PathBuf, SqlitePool)>,
}

impl PoolCache {
    pub(crate) fn get(&mut self, path: &Path) -> Option<&SqlitePool> {
        let position = self.entries.iter().position(|(key, _)| key == path)?;
        let entry = self.entries.remove(position)?;
        self.entries.push_back(entry);
        self.entries.back().map(|(_, pool)| pool)
    }

    pub(crate) fn insert(&mut self, path: PathBuf, pool: SqlitePool) {
        if let Some(position) = self.entries.iter().position(|(key, _)| key == &path) {
            self.entries.remove(position);
        }
        if self.entries.len() >= RETAINED_POOLS {
            self.entries.pop_front();
        }
        self.entries.push_back((path, pool));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    #[tokio::test]
    async fn eviction_is_lru_and_does_not_close_active_callers() {
        let mut cache = PoolCache::default();
        let active = SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap();
        cache.insert("active".into(), active.clone());
        for i in 0..RETAINED_POOLS {
            cache.insert(
                format!("pool-{i}").into(),
                SqlitePoolOptions::new()
                    .connect_lazy("sqlite::memory:")
                    .unwrap(),
            );
        }
        assert!(cache.get(Path::new("active")).is_none());
        assert!(!active.is_closed());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT 1")
                .fetch_one(&active)
                .await
                .unwrap(),
            1
        );
        assert!(cache.get(Path::new("pool-0")).is_some());
        cache.insert("next".into(), active.clone());
        assert!(cache.get(Path::new("pool-1")).is_none());
        assert!(cache.get(Path::new("pool-0")).is_some());
        assert_eq!(cache.entries.len(), RETAINED_POOLS);
    }
}
