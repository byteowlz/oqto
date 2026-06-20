//! Service layer for chat history - business logic, caching, and search.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use once_cell::sync::Lazy;
use tokio::sync::RwLock;

use crate::markdown;

use super::models::ChatMessage;
use super::repository::{
    default_legacy_data_dir, get_session_messages_from_dir, get_session_messages_parallel,
};

// Simple in-memory cache for session messages
static MESSAGE_CACHE: Lazy<Arc<RwLock<HashMap<String, (Vec<ChatMessage>, std::time::Instant)>>>> =
    Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

const CACHE_TTL_SECS: u64 = 30; // Cache messages for 30 seconds

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
            entries.sort_by_key(|e| Reverse(e.1));
            for (key, _) in entries.into_iter().skip(50) {
                cache.remove(&key);
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
                .filter_map(move |(part_idx, part)| {
                    if part.part_type != "text" {
                        return None;
                    }
                    part.text
                        .as_ref()
                        .map(|text| (msg_idx, part_idx, text.clone()))
                })
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
