//! Authorized workspace raster previews over the shared bounded media renderer.
use crate::{Config, FileServerError};
use axum::{
    body::Body,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use cap_std::fs::OpenOptionsExt;
use std::{
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::Duration,
};
static WORKERS: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(2)));
const SOURCE_LIMIT: u64 = 32 * 1024 * 1024;

pub(crate) async fn image(
    root: PathBuf,
    path: PathBuf,
    size: u32,
    config: &Config,
) -> Result<Response, FileServerError> {
    let cache = if cfg!(test) {
        None
    } else {
        let mut cache = oqto_media::cache::config().map_err(invalid)?.clone();
        if let Some(directory) = &config.thumbnail_cache_dir {
            cache.directory = std::path::absolute(directory)?.join("media-v1");
        }
        if std::env::var_os("OQTO_MEDIA_CACHE_IDLE_TTL_SECS").is_none() {
            cache.idle_ttl = Duration::from_secs(config.max_thumbnail_age);
            if config.max_thumbnail_age == 0 {
                cache.enabled = false;
            }
        }
        Some(cache)
    };
    let permit = WORKERS.clone().acquire_owned().await.map_err(invalid)?;
    let value = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        derive(&root, &path, size, cache.as_ref())
    })
    .await
    .map_err(invalid)??;
    let bytes = STANDARD
        .decode(
            value["data_base64"]
                .as_str()
                .ok_or_else(|| invalid("missing preview data"))?,
        )
        .map_err(invalid)?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/jpeg".to_owned()),
            (header::CONTENT_LENGTH, bytes.len().to_string()),
            (header::CACHE_CONTROL, "private, no-cache".to_owned()),
            (
                header::ETAG,
                format!(
                    "\"{}\"",
                    value["sha256"]
                        .as_str()
                        .ok_or_else(|| invalid("missing preview digest"))?
                ),
            ),
        ],
        Body::from(bytes),
    )
        .into_response())
}
fn invalid(error: impl std::fmt::Display) -> FileServerError {
    FileServerError::InvalidPath(error.to_string())
}
fn signature(meta: &std::fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        meta.dev(),
        meta.ino(),
        meta.len(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec(),
    )
}
fn derive(
    root: &Path,
    path: &Path,
    size: u32,
    cache: Option<&oqto_media::cache::Config>,
) -> Result<serde_json::Value, FileServerError> {
    let root = root.canonicalize()?;
    let relative = path
        .strip_prefix(&root)
        .map_err(|_| FileServerError::PathTraversal)?;
    let dir = cap_std::fs::Dir::open_ambient_dir(&root, cap_std::ambient_authority())?;
    let mut options = cap_std::fs::OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let mut file = dir.open_with(relative, &options)?.into_std();
    let before = file.metadata()?;
    if !before.is_file() {
        return Err(FileServerError::NotAFile);
    }
    if before.len() > SOURCE_LIMIT {
        return Err(FileServerError::FileTooLarge {
            size: before.len(),
            limit: SOURCE_LIMIT,
        });
    }
    let mut bytes = Vec::new();
    (&mut file).take(SOURCE_LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > SOURCE_LIMIT {
        return Err(FileServerError::FileTooLarge {
            size: bytes.len() as u64,
            limit: SOURCE_LIMIT,
        });
    }
    let result = oqto_media::thumbnail(&bytes, size, cache).map_err(invalid)?;
    // Reopen through the pinned directory capability: a warm cache cannot bypass
    // source changes or a symlink/ancestor replacement after admission.
    let current = dir.open_with(relative, &options)?.into_std();
    if signature(&before) != signature(&file.metadata()?)
        || signature(&before) != signature(&current.metadata()?)
    {
        return Err(invalid("image source changed during preview"));
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn thumbnail_http_contract_is_private_bounded_and_keeps_original()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().canonicalize()?;
        let source = root.join("image.png");
        image::DynamicImage::new_rgb8(1200, 900).save(&source)?;
        let original = std::fs::read(&source)?;
        let response = crate::handlers::get_thumbnail(
            axum::extract::State(crate::AppState::new(root)),
            axum::extract::Query(crate::handlers::ThumbnailQuery {
                directory: None,
                path: "image.png".into(),
                size: 512,
            }),
        )
        .await?;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "private, no-cache"
        );
        let bytes = axum::body::to_bytes(response.into_body(), 192 * 1024).await?;
        assert!(bytes.len() <= 128 * 1024);
        assert_eq!(image::load_from_memory(&bytes)?.width(), 512);
        assert_eq!(std::fs::read(&source)?, original);
        Ok(())
    }

    #[test]
    fn warm_cache_tracks_content_not_file_path() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().canonicalize()?;
        let source = root.join("image.png");
        let cache = oqto_media::cache::Config {
            enabled: true,
            directory: root.join("cache"),
            max_bytes: 1024 * 1024,
            idle_ttl: Duration::from_secs(60),
        };
        image::DynamicImage::new_rgb8(1000, 750).save(&source)?;
        let first = derive(&root, &source, 256, Some(&cache))?;
        assert_eq!(first, derive(&root, &source, 256, Some(&cache))?);
        image::RgbImage::from_pixel(1000, 750, image::Rgb([255, 0, 0])).save(&source)?;
        let second = derive(&root, &source, 256, Some(&cache))?;
        assert_ne!(first["source_sha256"], second["source_sha256"]);
        assert_ne!(first["sha256"], second["sha256"]);
        assert_eq!(std::fs::read_dir(&cache.directory)?.count(), 2);
        Ok(())
    }

    #[test]
    fn raster_preview_preserves_original_and_refuses_escape()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().canonicalize()?;
        let source = root.join("image.png");
        image::DynamicImage::new_rgb8(1200, 900).save(&source)?;
        let original = std::fs::read(&source)?;
        let preview = derive(&root, &source, 256, None)?;
        assert_eq!(preview["width"], 256);
        assert_eq!(preview["mime_type"], "image/jpeg");
        assert_eq!(std::fs::read(&source)?, original);
        assert!(derive(&root, &source, 100000, None).is_err());
        let outside = tempfile::NamedTempFile::new()?;
        std::os::unix::fs::symlink(outside.path(), root.join("escape.png"))?;
        assert!(derive(&root, &root.join("escape.png"), 256, None).is_err());
        assert!(derive(&root, outside.path(), 256, None).is_err());
        Ok(())
    }
}
