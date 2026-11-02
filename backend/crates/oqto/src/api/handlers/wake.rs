//! Wake-phrase release serving (waqy, `oqto-623a`).
//!
//! A release is an immutable folder (`manifest.json` + ONNX files, each with a
//! sha256). The browser engine loads it over these routes. Every file is checked
//! against its manifest hash before it is served; a mismatch refuses the release.
//! Weights never live in this repository: the folder path is operator config.
//! A detection in the browser is evidence only, never authentication or permission.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use axum::{
    Json,
    extract::{Path as UrlPath, State},
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::api::error::{ApiError, ApiResult};
use crate::api::state::AppState;

/// Release manifest as written by waqy (`src/detector.rs::Manifest`).
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub release: String,
    pub threshold: f32,
    pub classifiers: Vec<String>,
    pub files: BTreeMap<String, ManifestFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestFile {
    pub file: String,
    pub sha256: String,
}

/// What the browser needs: names, threshold and hashes, never filesystem paths.
#[derive(Debug, Serialize, PartialEq)]
pub struct PublicManifest {
    pub release: String,
    pub threshold: f32,
    pub classifiers: Vec<String>,
    pub files: BTreeMap<String, PublicFile>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PublicFile {
    pub sha256: String,
}

const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;

/// Load and validate a release folder; every listed file must match its hash.
pub fn load_verified(dir: &Path) -> anyhow::Result<Manifest> {
    use anyhow::{Context, bail, ensure};
    let text = std::fs::read_to_string(dir.join("manifest.json"))
        .with_context(|| format!("reading {}/manifest.json", dir.display()))?;
    let manifest: Manifest = serde_json::from_str(&text).context("parsing wake manifest")?;
    ensure!(
        manifest.threshold > 0.0 && manifest.threshold < 1.0,
        "wake threshold must be in (0, 1)"
    );
    for key in ["mel", "embedding"] {
        ensure!(
            manifest.files.contains_key(key),
            "manifest has no '{key}' file"
        );
    }
    ensure!(
        !manifest.classifiers.is_empty(),
        "manifest lists no classifiers"
    );
    for phrase in &manifest.classifiers {
        ensure!(
            manifest.files.contains_key(phrase),
            "manifest lists classifier '{phrase}' without a file"
        );
    }
    for key in manifest.files.keys() {
        read_verified(dir, &manifest, key)?;
    }
    if manifest.files.keys().any(|k| k.contains('/')) {
        bail!("manifest keys must not contain '/'");
    }
    Ok(manifest)
}

/// Bytes of one listed file after checking its sha256 (a release is immutable).
pub fn read_verified(dir: &Path, manifest: &Manifest, key: &str) -> anyhow::Result<Vec<u8>> {
    use anyhow::{Context, anyhow, ensure};
    let entry = manifest
        .files
        .get(key)
        .ok_or_else(|| anyhow!("no file '{key}' in manifest"))?;
    ensure!(
        !entry.file.is_empty()
            && !entry.file.contains('/')
            && !entry.file.contains('\\')
            && entry.file != ".."
            && !entry.file.starts_with('.'),
        "manifest file name '{}' is not a plain file name",
        entry.file
    );
    let path = dir.join(&entry.file);
    let meta =
        std::fs::symlink_metadata(&path).with_context(|| format!("reading {}", path.display()))?;
    ensure!(
        meta.is_file() && meta.len() <= MAX_FILE_BYTES,
        "{} must be a regular file of at most {MAX_FILE_BYTES} bytes",
        entry.file
    );
    let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let actual = hex::encode(Sha256::digest(&bytes));
    ensure!(
        actual.eq_ignore_ascii_case(&entry.sha256),
        "{} does not match its manifest sha256; refusing the release",
        entry.file
    );
    Ok(bytes)
}

pub fn public(manifest: &Manifest) -> PublicManifest {
    PublicManifest {
        release: manifest.release.clone(),
        threshold: manifest.threshold,
        classifiers: manifest.classifiers.clone(),
        files: manifest
            .files
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    PublicFile {
                        sha256: v.sha256.to_ascii_lowercase(),
                    },
                )
            })
            .collect(),
    }
}

fn release_dir(state: &AppState) -> Result<PathBuf, ApiError> {
    state
        .voice
        .wake_release
        .clone()
        .ok_or_else(|| ApiError::not_found("no wake release is configured"))
}

fn refuse(error: anyhow::Error) -> ApiError {
    tracing::warn!("wake release refused: {error:#}");
    ApiError::service_unavailable("the configured wake release failed verification")
}

/// GET /api/voice/wake/manifest
pub async fn wake_manifest(State(state): State<AppState>) -> ApiResult<Json<PublicManifest>> {
    let dir = release_dir(&state)?;
    let manifest = tokio::task::spawn_blocking(move || load_verified(&dir))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(refuse)?;
    Ok(Json(public(&manifest)))
}

/// GET /api/voice/wake/files/{key}
pub async fn wake_file(
    State(state): State<AppState>,
    UrlPath(key): UrlPath<String>,
) -> ApiResult<Response> {
    let dir = release_dir(&state)?;
    let bytes = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<Vec<u8>>> {
        let manifest = load_verified(&dir)?;
        if !manifest.files.contains_key(&key) {
            return Ok(None);
        }
        read_verified(&dir, &manifest, &key).map(Some)
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))?
    .map_err(refuse)?
    .ok_or_else(|| ApiError::not_found("no such file in the wake release"))?;
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-cache"),
    );
    Ok(response)
}

#[cfg(test)]
mod tests;
