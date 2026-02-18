//! Service layer for chat history - business logic, caching, and search.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use tokio::sync::RwLock;

use crate::markdown;

use super::models::{ChatMessage, HstryJsonResponse, HstrySearchHit};
use super::repository::{
    default_legacy_data_dir, get_session_messages_from_dir, get_session_messages_from_hstry,
    get_session_messages_parallel, get_session_messages_via_grpc, hstry_db_path,
};

// Simple in-memory cache for session messages
static MESSAGE_CACHE: Lazy<Arc<RwLock<HashMap<String, (Vec<ChatMessage>, std::time::Instant)>>>> =
    Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

const CACHE_TTL_SECS: u64 = 30; // Cache messages for 30 seconds

fn resolve_hstry_path() -> String {
    if let Ok(path) = env::var("HSTRY_PATH") {
        return path;
    }
    if let Ok(home) = env::var("HOME") {
        let local_bin = PathBuf::from(&home).join(".local/bin/hstry");
        if local_bin.exists() {
            return local_bin.to_string_lossy().to_string();
        }
    }
    "hstry".to_string()
}

/// Run a fast hstry search via the CLI (prefers the hstry service if enabled).
pub async fn search_hstry(query: &str, limit: usize) -> Result<Vec<HstrySearchHit>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let limit = limit.max(1);
    let hstry_path = resolve_hstry_path();
    let output = tokio::process::Command::new(&hstry_path)
        .arg("search")
        .arg(query)
        .arg("--limit")
        .arg(limit.to_string())
        .arg("--scope")
        .arg("local")
        .arg("--json")
        .env("HOME", env::var("HOME").unwrap_or_default())
        .output()
        .await
        .with_context(|| format!("Failed to execute hstry at '{hstry_path}'"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = if stderr.trim().is_empty() {
            "hstry search failed".to_string()
        } else {
            stderr.trim().to_string()
        };
        anyhow::bail!("{message}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok(Vec::new());
    }

    let response: HstryJsonResponse<Vec<HstrySearchHit>> =
        serde_json::from_str(&stdout).context("Failed to parse hstry search output")?;
    if !response.ok {
        let error = response
            .error
            .unwrap_or_else(|| "hstry search failed".to_string());
        anyhow::bail!(error);
    }

    Ok(response.result.unwrap_or_default())
}

/// Get all messages for a session (async version with caching).
pub async fn get_session_messages_async(session_id: &str) -> Result<Vec<ChatMessage>> {
    // Check cache first
    {
        let cache = MESSAGE_CACHE.read().await;
        if let Some((messages, timestamp)) = cache.get(session_id)
            && timestamp.elapsed().as_secs() < CACHE_TTL_SECS
        {
            tracing::debug!("Cache hit for session {}", session_id);
            return Ok(messages.clone());
        }
    }

    // Cache miss - try hstry DB first if available
    if let Some(db_path) = hstry_db_path() {
        match get_session_messages_from_hstry(session_id, &db_path).await {
            Ok(messages) if !messages.is_empty() => {
                let mut cache = MESSAGE_CACHE.write().await;
                cache.insert(
                    session_id.to_string(),
                    (messages.clone(), std::time::Instant::now()),
                );
                return Ok(messages);
            }
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(
                    session_id = %session_id,
                    error = %err,
                    "Failed to load messages from hstry DB, falling back to disk"
                );
            }
        }
    }

    // Cache miss - load from disk
    let legacy_data_dir = default_legacy_data_dir();
    let messages = get_session_messages_parallel(session_id, &legacy_data_dir).await?;

    // Update cache
    {
        let mut cache = MESSAGE_CACHE.write().await;
        cache.insert(
            session_id.to_string(),
            (messages.clone(), std::time::Instant::now()),
        );

        // Prune old entries (keep max 50)
        if cache.len() > 50 {
            let mut entries: Vec<_> = cache.iter().map(|(k, (_, t))| (k.clone(), *t)).collect();
            entries.sort_by(|a, b| b.1.cmp(&a.1)); // Sort by time descending
            for (key, _) in entries.into_iter().skip(50) {
                cache.remove(&key);
            }
        }
    }

    Ok(messages)
}

async fn get_session_messages_rendered_from_hstry(
    session_id: &str,
    db_path: &std::path::Path,
) -> Result<Vec<ChatMessage>> {
    let mut messages = get_session_messages_from_hstry(session_id, db_path).await?;
    for message in &mut messages {
        for part in &mut message.parts {
            if let Some(text) = &part.text {
                part.text_html = Some(markdown::render_markdown(text).await);
            }
        }
    }
    Ok(messages)
}

/// Get all messages for a session with pre-rendered markdown HTML.
///
/// This is useful for initial load of completed conversations.
/// During streaming, clients should use raw markdown and render client-side.
pub async fn get_session_messages_rendered(session_id: &str) -> Result<Vec<ChatMessage>> {
    if let Some(db_path) = hstry_db_path() {
        match get_session_messages_rendered_from_hstry(session_id, &db_path).await {
            Ok(messages) if !messages.is_empty() => return Ok(messages),
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(
                    session_id = %session_id,
                    error = %err,
                    "Failed to render messages from hstry DB, falling back to disk"
                );
            }
        }
    }
    get_session_messages_rendered_from_dir(session_id, &default_legacy_data_dir()).await
}

/// Get all messages for a session via gRPC (async version with caching).
pub async fn get_session_messages_via_grpc_cached(
    client: &crate::hstry::HstryClient,
    session_id: &str,
) -> Result<Vec<ChatMessage>> {
    // Check cache first
    {
        let cache = MESSAGE_CACHE.read().await;
        if let Some((messages, timestamp)) = cache.get(session_id)
            && timestamp.elapsed().as_secs() < CACHE_TTL_SECS
        {
            tracing::debug!("Cache hit for session {}", session_id);
            return Ok(messages.clone());
        }
    }

    // Cache miss - fetch via gRPC
    match get_session_messages_via_grpc(client, session_id).await {
        Ok(messages) if !messages.is_empty() => {
            let mut cache = MESSAGE_CACHE.write().await;
            cache.insert(
                session_id.to_string(),
                (messages.clone(), std::time::Instant::now()),
            );
            return Ok(messages);
        }
        Ok(_) => {}
        Err(err) => {
            tracing::warn!(
                session_id = %session_id,
                error = %err,
                "Failed to load messages from hstry gRPC, falling back to disk"
            );
        }
    }

    // Fallback to disk
    let legacy_data_dir = default_legacy_data_dir();
    let messages = get_session_messages_parallel(session_id, &legacy_data_dir).await?;

    // Update cache
    {
        let mut cache = MESSAGE_CACHE.write().await;
        cache.insert(
            session_id.to_string(),
            (messages.clone(), std::time::Instant::now()),
        );

        // Prune old entries (keep max 50)
        if cache.len() > 50 {
            let mut entries: Vec<_> = cache.iter().map(|(k, (_, t))| (k.clone(), *t)).collect();
            entries.sort_by(|a, b| b.1.cmp(&a.1));
            for (key, _) in entries.into_iter().skip(50) {
                cache.remove(&key);
            }
        }
    }

    Ok(messages)
}

/// Get all messages for a session via gRPC with pre-rendered markdown HTML.
pub async fn get_session_messages_rendered_via_grpc(
    client: &crate::hstry::HstryClient,
    session_id: &str,
) -> Result<Vec<ChatMessage>> {
    match get_session_messages_via_grpc(client, session_id).await {
        Ok(mut messages) if !messages.is_empty() => {
            for message in &mut messages {
                for part in &mut message.parts {
                    if let Some(text) = &part.text {
                        part.text_html = Some(markdown::render_markdown(text).await);
                    }
                }
            }
            return Ok(messages);
        }
        Ok(_) => {}
        Err(err) => {
            tracing::warn!(
                session_id = %session_id,
                error = %err,
                "Failed to render messages from hstry gRPC, falling back to disk"
            );
        }
    }
    get_session_messages_rendered_from_dir(session_id, &default_legacy_data_dir()).await
}

/// Get all messages for a session with pre-rendered markdown HTML from a specific directory.
pub async fn get_session_messages_rendered_from_dir(
    session_id: &str,
    legacy_data_dir: &std::path::Path,
) -> Result<Vec<ChatMessage>> {
    let mut messages = get_session_messages_from_dir(session_id, legacy_data_dir)?;

    // Collect all text content that needs rendering
    let texts_to_render: Vec<(usize, usize, String)> = messages
        .iter()
        .enumerate()
        .flat_map(|(msg_idx, msg)| {
            msg.parts
                .iter()
                .enumerate()
                .filter(|(_, part)| part.part_type == "text" && part.text.is_some())
                .map(move |(part_idx, part)| (msg_idx, part_idx, part.text.clone().unwrap()))
        })
        .collect();

    if texts_to_render.is_empty() {
        return Ok(messages);
    }

    // Render all markdown in parallel
    let contents: Vec<String> = texts_to_render
        .iter()
        .map(|(_, _, text)| text.clone())
        .collect();

    let rendered = markdown::render_markdown_batch(contents).await;

    // Apply rendered HTML back to messages
    for ((msg_idx, part_idx, _), html) in texts_to_render.into_iter().zip(rendered) {
        messages[msg_idx].parts[part_idx].text_html = Some(html);
    }

    Ok(messages)
}
