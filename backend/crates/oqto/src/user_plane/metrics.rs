use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;

use super::{
    DirEntry, FileContent, FileStat, MainChatMessage, MainChatSessionInfo, MemorySearchResults,
    SessionInfo, StartSessionRequest, StartSessionResponse, UserPlane,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UserPlanePath {
    Runner,
    Direct,
}

impl std::fmt::Display for UserPlanePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Runner => write!(f, "runner"),
            Self::Direct => write!(f, "direct"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UserPlaneMetricRow {
    pub path: UserPlanePath,
    pub op: String,
    pub requests: u64,
    pub errors: u64,
}

#[derive(Debug, Default)]
pub struct UserPlaneMetrics {
    counters: Mutex<HashMap<(UserPlanePath, &'static str), (u64, u64)>>,
}

impl UserPlaneMetrics {
    pub fn record_request(&self, path: UserPlanePath, op: &'static str) {
        if let Ok(mut guard) = self.counters.lock() {
            let entry = guard.entry((path, op)).or_insert((0, 0));
            entry.0 = entry.0.saturating_add(1);
        }
    }

    pub fn record_error(&self, path: UserPlanePath, op: &'static str) {
        if let Ok(mut guard) = self.counters.lock() {
            let entry = guard.entry((path, op)).or_insert((0, 0));
            entry.1 = entry.1.saturating_add(1);
        }
    }

    pub fn snapshot(&self) -> Vec<UserPlaneMetricRow> {
        let mut rows = if let Ok(guard) = self.counters.lock() {
            guard
                .iter()
                .map(|((path, op), (requests, errors))| UserPlaneMetricRow {
                    path: *path,
                    op: (*op).to_string(),
                    requests: *requests,
                    errors: *errors,
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        rows.sort_by(|a, b| {
            a.path
                .to_string()
                .cmp(&b.path.to_string())
                .then(a.op.cmp(&b.op))
        });
        rows
    }
}

#[derive(Clone)]
pub struct MeteredUserPlane {
    inner: Arc<dyn UserPlane>,
    path: UserPlanePath,
    metrics: Arc<UserPlaneMetrics>,
}

impl MeteredUserPlane {
    pub fn new(
        inner: Arc<dyn UserPlane>,
        path: UserPlanePath,
        metrics: Arc<UserPlaneMetrics>,
    ) -> Self {
        Self {
            inner,
            path,
            metrics,
        }
    }

    async fn metered<T, F>(&self, op: &'static str, fut: F) -> Result<T>
    where
        F: Future<Output = Result<T>>,
    {
        self.metrics.record_request(self.path, op);
        match fut.await {
            Ok(value) => Ok(value),
            Err(err) => {
                self.metrics.record_error(self.path, op);
                Err(err)
            }
        }
    }
}

#[async_trait]
impl UserPlane for MeteredUserPlane {
    async fn read_file(
        &self,
        path: &std::path::Path,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> Result<FileContent> {
        self.metered("read_file", self.inner.read_file(path, offset, limit))
            .await
    }

    async fn write_file(
        &self,
        path: &std::path::Path,
        content: &[u8],
        create_parents: bool,
    ) -> Result<()> {
        self.metered(
            "write_file",
            self.inner.write_file(path, content, create_parents),
        )
        .await
    }

    async fn list_directory(
        &self,
        path: &std::path::Path,
        include_hidden: bool,
    ) -> Result<Vec<DirEntry>> {
        self.metered(
            "list_directory",
            self.inner.list_directory(path, include_hidden),
        )
        .await
    }

    async fn stat(&self, path: &std::path::Path) -> Result<FileStat> {
        self.metered("stat", self.inner.stat(path)).await
    }

    async fn delete_path(&self, path: &std::path::Path, recursive: bool) -> Result<()> {
        self.metered("delete_path", self.inner.delete_path(path, recursive))
            .await
    }

    async fn create_directory(&self, path: &std::path::Path, create_parents: bool) -> Result<()> {
        self.metered(
            "create_directory",
            self.inner.create_directory(path, create_parents),
        )
        .await
    }

    async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        self.metered("list_sessions", self.inner.list_sessions())
            .await
    }

    async fn get_session(&self, session_id: &str) -> Result<Option<SessionInfo>> {
        self.metered("get_session", self.inner.get_session(session_id))
            .await
    }

    async fn start_session(&self, request: StartSessionRequest) -> Result<StartSessionResponse> {
        self.metered("start_session", self.inner.start_session(request))
            .await
    }

    async fn stop_session(&self, session_id: &str) -> Result<()> {
        self.metered("stop_session", self.inner.stop_session(session_id))
            .await
    }

    async fn list_main_chat_sessions(&self) -> Result<Vec<MainChatSessionInfo>> {
        self.metered(
            "list_main_chat_sessions",
            self.inner.list_main_chat_sessions(),
        )
        .await
    }

    async fn get_main_chat_messages(
        &self,
        session_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<MainChatMessage>> {
        self.metered(
            "get_main_chat_messages",
            self.inner.get_main_chat_messages(session_id, limit),
        )
        .await
    }

    async fn search_memories(
        &self,
        query: &str,
        limit: usize,
        category: Option<&str>,
    ) -> Result<MemorySearchResults> {
        self.metered(
            "search_memories",
            self.inner.search_memories(query, limit, category),
        )
        .await
    }

    async fn add_memory(
        &self,
        content: &str,
        category: Option<&str>,
        importance: Option<u8>,
    ) -> Result<String> {
        self.metered(
            "add_memory",
            self.inner.add_memory(content, category, importance),
        )
        .await
    }

    async fn delete_memory(&self, memory_id: &str) -> Result<()> {
        self.metered("delete_memory", self.inner.delete_memory(memory_id))
            .await
    }
}
