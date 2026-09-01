use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use oqto_apps::{
    AppCandidateStatus, AppDirEntry, AppFileSource, AppFileStat, AppPackageErrorCode,
    AppSourceError, BundleLimits, discover_candidates, snapshot_bundle,
};

const MANIFEST: &str = r#"
schema = "oqto-app/v0"
id = "hello-oqto"
version = "0.1.0"
title = { en = "Hello Oqto", de = "Hallo Oqto" }
description = "Zero-capability acceptance App"
presentations = ["sandboxed-web"]
requested_capabilities = []
bindings = ["work-directory"]
default_binding = "work-directory"

[presentation.sandboxed-web]
entry = "bundle/index.html"

[instance_state]
versioned = false

[assets]
max_bytes = 100000
"#;

#[derive(Debug, Clone)]
enum Node {
    Directory { modified_at: i64 },
    File { bytes: Vec<u8>, modified_at: i64 },
    Symlink,
}

#[derive(Clone, Default)]
struct FakeSource {
    nodes: Arc<Mutex<BTreeMap<PathBuf, Node>>>,
}

impl FakeSource {
    fn insert_dir(&self, path: impl Into<PathBuf>) {
        self.nodes
            .lock()
            .expect("fake source lock")
            .insert(path.into(), Node::Directory { modified_at: 1 });
    }

    fn insert_file(&self, path: impl Into<PathBuf>, bytes: impl Into<Vec<u8>>) {
        self.nodes.lock().expect("fake source lock").insert(
            path.into(),
            Node::File {
                bytes: bytes.into(),
                modified_at: 1,
            },
        );
    }

    fn insert_symlink(&self, path: impl Into<PathBuf>) {
        self.nodes
            .lock()
            .expect("fake source lock")
            .insert(path.into(), Node::Symlink);
    }

    fn hello() -> Self {
        let source = Self::default();
        source.insert_dir("oqto-apps");
        source.insert_dir("oqto-apps/hello-oqto.oqtoapp");
        source.insert_file(
            "oqto-apps/hello-oqto.oqtoapp/oqto-app.toml",
            MANIFEST.as_bytes(),
        );
        source.insert_dir("oqto-apps/hello-oqto.oqtoapp/bundle");
        source.insert_dir("oqto-apps/hello-oqto.oqtoapp/bundle/assets");
        source.insert_file(
            "oqto-apps/hello-oqto.oqtoapp/bundle/index.html",
            b"<!doctype html><script src=\"./assets/app.js\"></script>".as_slice(),
        );
        source.insert_file(
            "oqto-apps/hello-oqto.oqtoapp/bundle/assets/app.js",
            b"document.body.append('hello')".as_slice(),
        );
        source
    }

    fn stat_node(node: &Node) -> AppFileStat {
        match node {
            Node::Directory { modified_at } => AppFileStat {
                exists: true,
                is_file: false,
                is_dir: true,
                is_symlink: false,
                size: 0,
                modified_at: *modified_at,
            },
            Node::File { bytes, modified_at } => AppFileStat {
                exists: true,
                is_file: true,
                is_dir: false,
                is_symlink: false,
                size: u64::try_from(bytes.len()).expect("test file length"),
                modified_at: *modified_at,
            },
            Node::Symlink => AppFileStat {
                exists: true,
                is_file: false,
                is_dir: false,
                is_symlink: true,
                size: 0,
                modified_at: 1,
            },
        }
    }
}

#[async_trait]
impl AppFileSource for FakeSource {
    async fn list_directory(
        &self,
        relative_path: &Path,
    ) -> Result<Option<Vec<AppDirEntry>>, AppSourceError> {
        let nodes = self.nodes.lock().map_err(|_| AppSourceError::new("lock"))?;
        let Some(Node::Directory { .. }) = nodes.get(relative_path) else {
            return Ok(None);
        };
        let mut entries = Vec::new();
        for (path, node) in nodes.iter() {
            if path.parent() != Some(relative_path) {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                return Err(AppSourceError::new("non-UTF8 test path"));
            };
            let stat = Self::stat_node(node);
            entries.push(AppDirEntry {
                name: name.to_owned(),
                is_file: stat.is_file,
                is_dir: stat.is_dir,
                is_symlink: stat.is_symlink,
                size: stat.size,
                modified_at: stat.modified_at,
            });
        }
        Ok(Some(entries))
    }

    async fn stat(&self, relative_path: &Path) -> Result<Option<AppFileStat>, AppSourceError> {
        let nodes = self.nodes.lock().map_err(|_| AppSourceError::new("lock"))?;
        Ok(nodes.get(relative_path).map(Self::stat_node))
    }

    async fn read_file(
        &self,
        relative_path: &Path,
        max_bytes: u64,
    ) -> Result<Option<Vec<u8>>, AppSourceError> {
        let nodes = self.nodes.lock().map_err(|_| AppSourceError::new("lock"))?;
        let Some(Node::File { bytes, .. }) = nodes.get(relative_path) else {
            return Ok(None);
        };
        let size = u64::try_from(bytes.len()).map_err(|_| AppSourceError::new("size"))?;
        if size > max_bytes {
            return Err(AppSourceError::new("bounded read exceeded"));
        }
        Ok(Some(bytes.clone()))
    }
}

#[tokio::test]
async fn discovers_only_direct_suffixed_packages() -> Result<(), Box<dyn std::error::Error>> {
    let source = FakeSource::hello();
    source.insert_dir("oqto-apps/ignored");
    source.insert_dir("oqto-apps/nested");
    source.insert_dir("oqto-apps/nested/hidden.oqtoapp");

    let candidates = discover_candidates(&source, BundleLimits::default()).await?;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].package_dir_name, "hello-oqto.oqtoapp");
    assert!(matches!(
        candidates[0].status,
        AppCandidateStatus::Publishable(_)
    ));
    Ok(())
}

#[tokio::test]
async fn rejects_symlinked_package_root_without_following_it()
-> Result<(), Box<dyn std::error::Error>> {
    let source = FakeSource::default();
    source.insert_dir("oqto-apps");
    source.insert_symlink("oqto-apps/evil.oqtoapp");

    let candidates = discover_candidates(&source, BundleLimits::default()).await?;
    let AppCandidateStatus::Rejected(error) = &candidates[0].status else {
        return Err("symlink package was publishable".into());
    };
    assert_eq!(error.code, AppPackageErrorCode::SymlinkRejected);
    Ok(())
}

#[tokio::test]
async fn snapshots_immutable_roots_only_and_has_deterministic_digest()
-> Result<(), Box<dyn std::error::Error>> {
    let source = FakeSource::hello();
    source.insert_file(
        "oqto-apps/hello-oqto.oqtoapp/src/not-runtime.ts",
        b"ignored source".as_slice(),
    );
    // `operations/` is content-addressed whenever it exists, even for an App
    // that requests no operations capability, so a later manifest change can
    // never widen what an already-pinned Definition contains.
    source.insert_dir("oqto-apps/hello-oqto.oqtoapp/operations");
    source.insert_file(
        "oqto-apps/hello-oqto.oqtoapp/operations/bridge",
        b"#!/bin/sh\n".as_slice(),
    );

    let first = snapshot_bundle(&source, "hello-oqto.oqtoapp", BundleLimits::default()).await?;
    let second = snapshot_bundle(&source, "hello-oqto.oqtoapp", BundleLimits::default()).await?;

    assert_eq!(first.digest, second.digest);
    assert_eq!(first.files.len(), 3);
    assert!(first.files.iter().all(|file| {
        file.relative_path.starts_with("bundle") || file.relative_path.starts_with("operations")
    }));
    assert!(
        !first
            .files
            .iter()
            .any(|file| file.relative_path.starts_with("src"))
    );

    // Editing an operation file yields a different Definition.
    source.insert_file(
        "oqto-apps/hello-oqto.oqtoapp/operations/bridge",
        b"#!/bin/sh\ncurl evil.example\n".as_slice(),
    );
    let tampered = snapshot_bundle(&source, "hello-oqto.oqtoapp", BundleLimits::default()).await?;
    assert_ne!(first.digest, tampered.digest);
    Ok(())
}

#[tokio::test]
async fn rejects_symlink_inside_bundle() {
    let source = FakeSource::hello();
    source.insert_symlink("oqto-apps/hello-oqto.oqtoapp/bundle/escape.js");

    let error = snapshot_bundle(&source, "hello-oqto.oqtoapp", BundleLimits::default())
        .await
        .expect_err("bundle symlink must fail");
    assert_eq!(error.code, AppPackageErrorCode::SymlinkRejected);
}

#[tokio::test]
async fn bundle_byte_limit_is_enforced() {
    let source = FakeSource::hello();
    let limits = BundleLimits {
        max_bundle_bytes: 8,
        ..BundleLimits::default()
    };
    let error = snapshot_bundle(&source, "hello-oqto.oqtoapp", limits)
        .await
        .expect_err("oversized bundle must fail");
    assert_eq!(error.code, AppPackageErrorCode::BundleTooLarge);
}
