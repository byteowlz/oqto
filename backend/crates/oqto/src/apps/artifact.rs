use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use oqto_apps::BundleSnapshot;
use tokio::fs;
use uuid::Uuid;

/// Backend-owned immutable Definition artifact storage.
#[derive(Debug, Clone)]
pub struct AppArtifactStore {
    root: PathBuf,
}

impl AppArtifactStore {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Atomically materialize one validated snapshot. An existing completed
    /// digest is reused; source workspace files are never served directly.
    pub async fn materialize(&self, snapshot: &BundleSnapshot) -> Result<PathBuf> {
        let definitions_root = self.root.join("definitions");
        fs::create_dir_all(&definitions_root)
            .await
            .with_context(|| {
                format!("creating App artifact root {}", definitions_root.display())
            })?;

        let final_dir = definitions_root.join(snapshot.digest.as_str());
        if fs::try_exists(&final_dir).await.unwrap_or(false) {
            self.verify_complete(&final_dir, snapshot.digest.as_str())
                .await?;
            return Ok(final_dir);
        }

        let temp_dir = definitions_root.join(format!(
            ".tmp-{}-{}",
            snapshot.digest.as_str(),
            Uuid::new_v4()
        ));
        fs::create_dir(&temp_dir)
            .await
            .with_context(|| format!("creating temporary App artifact {}", temp_dir.display()))?;

        let result = self.write_snapshot(&temp_dir, snapshot).await;
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&temp_dir).await;
            return Err(error);
        }

        match fs::rename(&temp_dir, &final_dir).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_dir_all(&temp_dir).await;
                self.verify_complete(&final_dir, snapshot.digest.as_str())
                    .await?;
            }
            Err(error) => {
                let _ = fs::remove_dir_all(&temp_dir).await;
                return Err(error).with_context(|| {
                    format!("atomically installing App artifact {}", final_dir.display())
                });
            }
        }

        Ok(final_dir)
    }

    #[must_use]
    pub fn definition_dir(&self, content_digest: &str) -> Option<PathBuf> {
        if content_digest.len() != 64
            || !content_digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return None;
        }
        Some(self.root.join("definitions").join(content_digest))
    }

    async fn write_snapshot(&self, directory: &Path, snapshot: &BundleSnapshot) -> Result<()> {
        fs::write(directory.join("oqto-app.toml"), &snapshot.manifest_bytes)
            .await
            .context("writing immutable App manifest")?;

        for file in &snapshot.files {
            validate_artifact_relative(&file.relative_path)?;
            let target = directory.join(&file.relative_path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).await.with_context(|| {
                    format!("creating App artifact directory {}", parent.display())
                })?;
            }
            fs::write(&target, &file.bytes)
                .await
                .with_context(|| format!("writing App artifact file {}", target.display()))?;
            #[cfg(unix)]
            if file.relative_path.starts_with("operations/")
                && file
                    .relative_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    != Some("table.toml")
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&target, std::fs::Permissions::from_mode(0o500))
                    .await
                    .with_context(|| {
                        format!("marking App operation executable {}", target.display())
                    })?;
            }
        }

        fs::write(directory.join(".complete"), snapshot.digest.as_str())
            .await
            .context("writing App artifact completion marker")?;
        Ok(())
    }

    async fn verify_complete(&self, directory: &Path, expected_digest: &str) -> Result<()> {
        let marker = fs::read_to_string(directory.join(".complete"))
            .await
            .with_context(|| {
                format!(
                    "reading App artifact completion marker in {}",
                    directory.display()
                )
            })?;
        if marker != expected_digest {
            bail!(
                "App artifact completion marker mismatch for {}",
                directory.display()
            );
        }
        Ok(())
    }
}

fn validate_artifact_relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("invalid App artifact relative path: {path:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use oqto_apps::{BundleFile, BundleSnapshot, parse_manifest};
    use tempfile::TempDir;

    use super::*;

    fn snapshot() -> BundleSnapshot {
        let manifest_bytes = br#"
schema = "oqto-app/v0"
id = "hello"
version = "0.1.0"
title = { en = "Hello" }
presentations = ["sandboxed-web"]
requested_capabilities = []
bindings = ["work-directory"]
default_binding = "work-directory"
[presentation.sandboxed-web]
entry = "bundle/index.html"
"#
        .to_vec();
        let files = vec![BundleFile {
            relative_path: PathBuf::from("bundle/index.html"),
            bytes: b"hello".to_vec(),
        }];
        let digest = oqto_apps::digest_bundle(
            &manifest_bytes,
            files
                .iter()
                .map(|file| (file.relative_path.as_path(), file.bytes.as_slice())),
        );
        BundleSnapshot {
            manifest: parse_manifest("hello.oqtoapp", &manifest_bytes, 65_536)
                .expect("valid fixture manifest"),
            manifest_bytes,
            files,
            digest,
            total_bytes: 5,
            operations: Vec::new(),
            agent_context: None,
        }
    }

    #[tokio::test]
    async fn materialization_is_atomic_and_idempotent() -> Result<()> {
        let temp = TempDir::new()?;
        let store = AppArtifactStore::new(temp.path().join("apps"));
        let snapshot = snapshot();

        let first = store.materialize(&snapshot).await?;
        let second = store.materialize(&snapshot).await?;
        assert_eq!(first, second);
        assert_eq!(fs::read(first.join("bundle/index.html")).await?, b"hello");
        assert_eq!(
            fs::read_to_string(first.join(".complete")).await?,
            snapshot.digest.as_str()
        );
        Ok(())
    }

    #[test]
    fn definition_dir_rejects_non_digest_input() {
        let store = AppArtifactStore::new(PathBuf::from("/tmp/apps"));
        assert!(store.definition_dir("../escape").is_none());
        assert!(store.definition_dir(&"A".repeat(64)).is_none());
    }
}
