//! API key management for external integrations.

mod models;
mod repository;

pub use models::{
    ApiKey, ApiKeyAuthUser, ApiKeyCreateRequest, ApiKeyCreateResponse, ApiKeyListItem,
};
pub use repository::{ApiKeyRepository, ApiKeyStoreError, ApiKeyStoreResult};

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

const API_KEY_PREFIX: &str = "octo_sk_";

/// Generate a new API key and its prefix.
pub fn generate_api_key() -> (String, String) {
    let suffix = loop {
        let bytes: [u8; 32] = rand::random();
        let mut candidate = URL_SAFE_NO_PAD.encode(bytes);
        candidate = candidate.replace('_', "-");
        if candidate.starts_with('-') || candidate.ends_with('-') || candidate.contains("--") {
            continue;
        }
        break candidate;
    };
    let full = format!("{API_KEY_PREFIX}{suffix}");
    let prefix = suffix.chars().take(8).collect::<String>();
    (full, prefix)
}

/// Hash a raw API key for storage/lookup.
pub fn hash_api_key(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// Normalize expires_at to RFC3339 (UTC) string.
pub fn normalize_expires_at(value: &str) -> Result<String> {
    let parsed = chrono::DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").map(|dt| {
                chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc)
            })
        })
        .with_context(|| format!("invalid expires_at timestamp: {value}"))?;
    Ok(parsed.to_rfc3339())
}

/// Parse a stored timestamp string into a DateTime (UTC) if possible.
pub fn parse_timestamp(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").map(|dt| {
                chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc)
            })
        })
        .ok()
}

/// Check if a raw key string looks like an API key.
pub fn is_api_key(raw: &str) -> bool {
    raw.starts_with(API_KEY_PREFIX)
}
