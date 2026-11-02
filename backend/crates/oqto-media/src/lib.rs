//! Shared derived display media. Callers retain all Session/file authorization
//! and must validate their source snapshot before publishing a result.
pub mod cache;
pub mod inline;
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::ImageReader;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::Cursor;

pub(crate) fn dimensions(bytes: &[u8], format: image::ImageFormat) -> Result<(u32, u32)> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(192 * 1024 * 1024);
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    reader.limits(limits);
    Ok(reader.into_dimensions()?)
}

/// A deterministic bounded JPEG preview. No paths, network, model or authority.
/// `None` deliberately disables caching (also useful for isolated unit tests).
pub fn thumbnail(bytes: &[u8], edge: u32, cache: Option<&cache::Config>) -> Result<Value> {
    ensure!((1..=768).contains(&edge), "unsupported media preview size");
    ensure!(
        bytes.len() <= 32 * 1024 * 1024,
        "preview source exceeds bound"
    );
    let format = image::guess_format(bytes)?;
    ensure!(
        matches!(
            format,
            image::ImageFormat::Png
                | image::ImageFormat::Jpeg
                | image::ImageFormat::Gif
                | image::ImageFormat::WebP
                | image::ImageFormat::Bmp
                | image::ImageFormat::Ico
        ),
        "unsupported raster format"
    );
    let (width, height) = dimensions(bytes, format)?;
    ensure!(
        width > 0
            && height > 0
            && width <= 16384
            && height <= 16384
            && u64::from(width) * u64::from(height) <= 40_000_000,
        "preview dimensions exceed bound"
    );
    let hash = hex::encode(Sha256::digest(bytes));
    let variant = format!("thumb-jpeg-fit{edge}-budget128k-v1");
    if let Some(cache) = cache {
        match cache.get_for(&hash, &variant, edge) {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) => tracing::warn!(%error,"media cache unavailable; deriving preview"),
        }
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(192 * 1024 * 1024);
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    reader.limits(limits);
    let decoded = reader.decode()?;
    let mut encoded = None;
    for (size, quality) in [
        (edge, 65),
        ((edge * 2 / 3).max(1), 55),
        ((edge / 2).max(1), 45),
    ] {
        let resized = decoded.thumbnail(size, size).to_rgb8();
        let mut data = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, quality)
            .encode_image(&resized)?;
        if data.len() <= 128 * 1024 {
            let (w, h) = resized.dimensions();
            encoded = Some((data, w, h));
            break;
        }
    }
    let (data, width, height) = encoded.context("thumbnail exceeds encoded bound")?;
    let value = json!({"source_sha256":hash,"sha256":hex::encode(Sha256::digest(&data)),"variant":variant,"mime_type":"image/jpeg","width":width,"height":height,"byte_size":data.len(),"data_base64":STANDARD.encode(data)});
    if let Some(cache) = cache
        && let Err(error) = cache.put_for(&hash, &variant, edge, &value)
    {
        tracing::warn!(%error,"media cache publication failed; serving preview");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requested_size_and_encoder_keys_are_distinct() -> Result<()> {
        let mut source = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(1200, 900).write_to(&mut source, image::ImageFormat::Png)?;
        let a = thumbnail(source.get_ref(), 128, None)?;
        let b = thumbnail(source.get_ref(), 512, None)?;
        assert_eq!(a["source_sha256"], b["source_sha256"]);
        assert_ne!(a["variant"], b["variant"]);
        assert_eq!(a["width"], 128);
        assert_eq!(b["width"], 512);
        assert!(thumbnail(source.get_ref(), 0, None).is_err());
        assert!(thumbnail(source.get_ref(), 769, None).is_err());
        Ok(())
    }
}
