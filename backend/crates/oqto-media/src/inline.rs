//! Wire projection and content lookup; neither operation confers authority.
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::io::Cursor;
#[cfg(test)]
mod tests;
pub const MAX_IMAGE: usize = 8 * 1024 * 1024;
#[derive(Clone, Debug, Serialize)]
pub struct MediaRef {
    pub sha256: String,
    pub mime_type: &'static str,
    pub width: u32,
    pub height: u32,
    pub byte_size: usize,
}
fn encoded(value: &Value) -> Option<&str> {
    if value["type"] != "image" {
        return None;
    }
    value["data"].as_str().or_else(|| {
        if value["source"]["type"] == "base64" {
            value["source"]["data"].as_str()
        } else {
            value["source"]["url"]
                .as_str()?
                .strip_prefix("data:")?
                .split_once(";base64,")
                .map(|(_, data)| data)
        }
    })
}
fn decode(data: &str) -> Result<Vec<u8>> {
    ensure!(
        data.len() <= MAX_IMAGE.div_ceil(3) * 4,
        "image exceeds source bound"
    );
    let bytes = STANDARD.decode(data).context("invalid image encoding")?;
    ensure!(bytes.len() <= MAX_IMAGE, "image exceeds source bound");
    Ok(bytes)
}
pub fn describe(bytes: &[u8]) -> Result<MediaRef> {
    ensure!(bytes.len() <= MAX_IMAGE, "image exceeds source bound");
    let format = image::guess_format(bytes)?;
    let mime_type = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => "image/webp",
        image::ImageFormat::Gif => "image/gif",
        _ => anyhow::bail!("unsupported image format"),
    };
    let (width, height) = crate::dimensions(bytes, format)?;
    ensure!(
        width > 0
            && height > 0
            && width <= 16384
            && height <= 16384
            && u64::from(width) * u64::from(height) <= 40_000_000,
        "image dimensions exceed bound"
    );
    Ok(MediaRef {
        sha256: hex::encode(Sha256::digest(bytes)),
        mime_type,
        width,
        height,
        byte_size: bytes.len(),
    })
}
pub fn full(bytes: &[u8]) -> Result<Value> {
    let r = describe(bytes)?;
    Ok(
        json!({"source_sha256":r.sha256,"sha256":r.sha256,"variant":"full","mime_type":r.mime_type,"width":r.width,"height":r.height,"byte_size":bytes.len(),"data_base64":STANDARD.encode(bytes)}),
    )
}
pub fn project(value: &mut Value) -> Result<()> {
    project_at(value, None, 0)
}
/// Add only an opaque public Session identity, never a host or filesystem path.
pub fn project_for_session(value: &mut Value, session: &str) -> Result<()> {
    ensure!(
        !session.is_empty()
            && session.len() <= 256
            && !session.starts_with("pending-")
            && !session.starts_with("tmp:"),
        "invalid public Session identity"
    );
    project_at(value, Some(session), 0)
}
fn project_at(value: &mut Value, session: Option<&str>, depth: usize) -> Result<()> {
    ensure!(depth <= 96, "history projection nesting exceeded");
    if let Some(data) = encoded(value) {
        let labels: [Option<Value>; 3] = ["id", "alt", "caption"]
            .map(|key| value.get(key).filter(|value| value.is_string()).cloned());
        *value = match decode(data).and_then(|b| describe(&b)) {
            Ok(reference) => {
                let mut value = json!({"type":"image","media_ref":reference});
                if let Some(session) = session {
                    value["media_ref"]["session_id"] = session.into();
                }
                value
            }
            Err(_) => json!({"type":"image","media_unavailable":"unsupported_or_oversized"}),
        };
        for (key, label) in ["id", "alt", "caption"].into_iter().zip(labels) {
            if let Some(label) = label {
                value[key] = label;
            }
        }
        return Ok(());
    }
    if value["type"] == "image"
        && value["media_ref"].is_object()
        && let Some(session) = session
    {
        value["media_ref"]["session_id"] = session.into();
    }
    match value {
        Value::Array(items) => {
            for item in items {
                project_at(item, session, depth + 1)?;
            }
        }
        Value::Object(fields) => {
            for (key, item) in fields {
                project_at(item, session, depth + 1)?;
                if key == "tool_output"
                    && let Value::Array(blocks) = item
                {
                    let mut seen = std::collections::HashSet::new();
                    blocks.retain(|block| match block["media_ref"]["sha256"].as_str() {
                        Some(hash) if block["type"] == "image" => seen.insert(hash.to_owned()),
                        _ => true,
                    });
                }
            }
        }
        _ => {}
    };
    Ok(())
}
pub fn find_image(value: &Value, hash: &str) -> Result<Option<Vec<u8>>> {
    ensure!(
        hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid media hash"
    );
    find_at(value, hash, 0)
}
fn find_at(value: &Value, hash: &str, depth: usize) -> Result<Option<Vec<u8>>> {
    ensure!(depth <= 96, "media nesting exceeded");
    if let Some(data) = encoded(value) {
        return Ok(decode(data)
            .ok()
            .filter(|b| hex::encode(Sha256::digest(b)) == hash));
    }
    let children: Box<dyn Iterator<Item = &Value>> = match value {
        Value::Array(items) => Box::new(items.iter()),
        Value::Object(fields) => Box::new(fields.values()),
        _ => return Ok(None),
    };
    for child in children {
        if let Some(found) = find_at(child, hash, depth + 1)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}
