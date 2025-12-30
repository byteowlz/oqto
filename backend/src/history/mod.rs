//! Chat history module - reads OpenCode session history from disk.
//!
//! This module provides read-only access to OpenCode chat sessions stored on disk,
//! without requiring a running OpenCode instance.
//!
//! OpenCode stores sessions in: ~/.local/share/opencode/storage/session/{projectID}/ses_*.json
//! where projectID is a hash of the workspace directory path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::markdown;

// Simple in-memory cache for session messages
static MESSAGE_CACHE: Lazy<Arc<RwLock<HashMap<String, (Vec<ChatMessage>, std::time::Instant)>>>> = 
    Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

const CACHE_TTL_SECS: u64 = 30; // Cache messages for 30 seconds

/// OpenCode session as stored on disk.
/// This matches the actual structure in ~/.local/share/opencode/storage/session/
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub version: Option<String>,
    pub title: Option<String>,
    #[serde(rename = "parentID")]
    pub parent_id: Option<String>,
    /// The workspace directory path
    pub directory: Option<String>,
    /// Project ID (hash of directory)
    #[serde(rename = "projectID")]
    pub project_id: Option<String>,
    pub time: SessionTime,
}

/// Session timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTime {
    pub created: i64,
    pub updated: i64,
}

/// A chat session with its project context.
#[derive(Debug, Clone, Serialize)]
pub struct ChatSession {
    /// Session ID (e.g., "ses_xxx")
    pub id: String,
    /// Session title
    pub title: Option<String>,
    /// Parent session ID (for child sessions)
    pub parent_id: Option<String>,
    /// Workspace/project path
    pub workspace_path: String,
    /// Project name (derived from path)
    pub project_name: String,
    /// Created timestamp (ms since epoch)
    pub created_at: i64,
    /// Updated timestamp (ms since epoch)
    pub updated_at: i64,
    /// OpenCode version that created this session
    pub version: Option<String>,
    /// Whether this session is a child session
    pub is_child: bool,
    /// Path to the session JSON file (for loading messages later)
    pub source_path: Option<String>,
}

/// Default OpenCode data directory.
fn default_opencode_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opencode")
}

/// Extract project name from workspace path.
pub fn project_name_from_path(path: &str) -> String {
    if path == "global" || path.is_empty() {
        return "Global".to_string();
    }
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// Read all chat sessions from OpenCode's data directory.
pub fn list_sessions() -> Result<Vec<ChatSession>> {
    list_sessions_from_dir(&default_opencode_data_dir())
}

/// Read all chat sessions from a specific OpenCode data directory.
/// 
/// OpenCode stores sessions in: {opencode_dir}/storage/session/{projectID}/ses_*.json
pub fn list_sessions_from_dir(opencode_dir: &Path) -> Result<Vec<ChatSession>> {
    let session_dir = opencode_dir.join("storage/session");
    
    if !session_dir.exists() {
        tracing::debug!("Session directory does not exist: {:?}", session_dir);
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();

    // Iterate over project hash directories
    let project_entries = std::fs::read_dir(&session_dir)
        .with_context(|| format!("reading session dir: {:?}", session_dir))?;

    for project_entry in project_entries {
        let project_entry = match project_entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }

        // Read session files in this project directory
        let session_entries = match std::fs::read_dir(&project_path) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for session_entry in session_entries {
            let session_entry = match session_entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let session_path = session_entry.path();
            
            // Only process ses_*.json files
            // Only process ses_*.json files
            let is_session_file = session_path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|name| name.starts_with("ses_") && name.ends_with(".json"))
                .unwrap_or(false);
            
            if !is_session_file {
                continue;
            }
            
            // Skip if not a regular file
            if !session_path.is_file() {
                continue;
            }

            // Read and parse session info
            let content = match std::fs::read_to_string(&session_path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::debug!("Failed to read session file {:?}: {}", session_path, e);
                    continue;
                }
            };

            let info: SessionInfo = match serde_json::from_str(&content) {
                Ok(i) => i,
                Err(e) => {
                    tracing::debug!("Failed to parse session file {:?}: {}", session_path, e);
                    continue;
                }
            };

            // Get workspace path from the session's directory field
            let workspace_path = info.directory.clone().unwrap_or_else(|| "global".to_string());
            let project_name = project_name_from_path(&workspace_path);
            let is_child = info.parent_id.is_some();

            sessions.push(ChatSession {
                id: info.id.clone(),
                title: info.title,
                parent_id: info.parent_id,
                workspace_path,
                project_name,
                created_at: info.time.created,
                updated_at: info.time.updated,
                version: info.version,
                is_child,
                source_path: Some(session_path.to_string_lossy().to_string()),
            });
        }
    }

    // Sort by updated_at descending (most recent first)
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    
    tracing::info!("Found {} sessions in {:?}", sessions.len(), session_dir);

    Ok(sessions)
}

/// List sessions grouped by project/workspace.
pub fn list_sessions_grouped() -> Result<HashMap<String, Vec<ChatSession>>> {
    let sessions = list_sessions()?;
    let mut grouped: HashMap<String, Vec<ChatSession>> = HashMap::new();

    for session in sessions {
        grouped
            .entry(session.workspace_path.clone())
            .or_default()
            .push(session);
    }

    Ok(grouped)
}

/// List sessions for a specific workspace path.
pub fn list_sessions_for_workspace(workspace_path: &str) -> Result<Vec<ChatSession>> {
    let sessions = list_sessions()?;
    Ok(sessions
        .into_iter()
        .filter(|s| s.workspace_path == workspace_path)
        .collect())
}

/// Get a single session by ID.
pub fn get_session(session_id: &str) -> Result<Option<ChatSession>> {
    let sessions = list_sessions()?;
    Ok(sessions.into_iter().find(|s| s.id == session_id))
}

// ============================================================================
// Message loading
// ============================================================================

/// Message metadata as stored in OpenCode's message directory.
#[derive(Debug, Clone, Deserialize)]
pub struct MessageInfo {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub role: String,
    pub time: MessageTime,
    #[serde(rename = "parentID")]
    pub parent_id: Option<String>,
    #[serde(rename = "modelID")]
    pub model_id: Option<String>,
    #[serde(rename = "providerID")]
    pub provider_id: Option<String>,
    pub agent: Option<String>,
    pub summary: Option<MessageSummary>,
    pub tokens: Option<TokenUsage>,
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageTime {
    pub created: i64,
    pub completed: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageSummary {
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenUsage {
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub reasoning: Option<i64>,
}

/// Message part as stored in OpenCode's part directory.
#[derive(Debug, Clone, Deserialize)]
pub struct PartInfo {
    pub id: String,
    #[serde(rename = "messageID")]
    pub message_id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "type")]
    pub part_type: String,
    /// Text content (for type="text")
    pub text: Option<String>,
    /// Tool name (for type="tool")
    pub tool: Option<String>,
    /// Tool call state (for type="tool")
    pub state: Option<ToolState>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolState {
    pub status: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<String>,
    pub title: Option<String>,
}

/// A chat message with its content parts.
#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub parent_id: Option<String>,
    pub model_id: Option<String>,
    pub provider_id: Option<String>,
    pub agent: Option<String>,
    pub summary_title: Option<String>,
    pub tokens_input: Option<i64>,
    pub tokens_output: Option<i64>,
    pub cost: Option<f64>,
    /// Message content parts
    pub parts: Vec<ChatMessagePart>,
}

/// A single part of a chat message.
#[derive(Debug, Clone, Serialize)]
pub struct ChatMessagePart {
    pub id: String,
    pub part_type: String,
    /// Text content (for text parts)
    pub text: Option<String>,
    /// Pre-rendered HTML (for text parts, when render=true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_html: Option<String>,
    /// Tool name (for tool parts)
    pub tool_name: Option<String>,
    /// Tool input (for tool parts)
    pub tool_input: Option<serde_json::Value>,
    /// Tool output (for tool parts)
    pub tool_output: Option<String>,
    /// Tool status (for tool parts)
    pub tool_status: Option<String>,
    /// Tool title/summary (for tool parts)
    pub tool_title: Option<String>,
}

/// Get all messages for a session (async version with caching).
pub async fn get_session_messages_async(session_id: &str) -> Result<Vec<ChatMessage>> {
    // Check cache first
    {
        let cache = MESSAGE_CACHE.read().await;
        if let Some((messages, timestamp)) = cache.get(session_id) {
            if timestamp.elapsed().as_secs() < CACHE_TTL_SECS {
                tracing::debug!("Cache hit for session {}", session_id);
                return Ok(messages.clone());
            }
        }
    }

    // Cache miss - load from disk
    let opencode_dir = default_opencode_data_dir();
    let messages = get_session_messages_parallel(session_id, &opencode_dir).await?;

    // Update cache
    {
        let mut cache = MESSAGE_CACHE.write().await;
        cache.insert(session_id.to_string(), (messages.clone(), std::time::Instant::now()));
        
        // Prune old entries (keep max 50)
        if cache.len() > 50 {
            let mut entries: Vec<_> = cache.iter()
                .map(|(k, (_, t))| (k.clone(), *t))
                .collect();
            entries.sort_by(|a, b| b.1.cmp(&a.1)); // Sort by time descending
            for (key, _) in entries.into_iter().skip(50) {
                cache.remove(&key);
            }
        }
    }

    Ok(messages)
}

/// Invalidate the cache for a session.
pub async fn invalidate_message_cache(session_id: &str) {
    let mut cache = MESSAGE_CACHE.write().await;
    cache.remove(session_id);
}

/// Get all messages for a session (sync version, for backwards compatibility).
pub fn get_session_messages(session_id: &str) -> Result<Vec<ChatMessage>> {
    get_session_messages_from_dir(session_id, &default_opencode_data_dir())
}

/// Get all messages for a session using parallel I/O.
async fn get_session_messages_parallel(session_id: &str, opencode_dir: &Path) -> Result<Vec<ChatMessage>> {
    let message_dir = opencode_dir.join("storage/message").join(session_id);
    let part_dir = opencode_dir.join("storage/part");

    if !message_dir.exists() {
        tracing::debug!("Message directory does not exist: {:?}", message_dir);
        return Ok(Vec::new());
    }

    // Read message directory entries
    let message_entries: Vec<_> = std::fs::read_dir(&message_dir)
        .with_context(|| format!("reading message dir: {:?}", message_dir))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .file_name()
                .and_then(|s| s.to_str())
                .map(|name| name.starts_with("msg_") && name.ends_with(".json"))
                .unwrap_or(false)
        })
        .collect();

    // Spawn tasks to read messages in parallel
    let mut tasks = Vec::with_capacity(message_entries.len());
    
    for entry in message_entries {
        let msg_path = entry.path();
        let part_dir = part_dir.clone();
        
        tasks.push(tokio::task::spawn_blocking(move || {
            load_single_message(&msg_path, &part_dir)
        }));
    }

    // Wait for all tasks and collect results
    let mut messages = Vec::new();
    for task in tasks {
        if let Ok(Ok(Some(msg))) = task.await {
            messages.push(msg);
        }
    }

    // Sort by created_at ascending (chronological order)
    messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    tracing::debug!(
        "Loaded {} messages for session {} using parallel I/O",
        messages.len(),
        session_id
    );

    Ok(messages)
}

/// Load a single message and its parts.
fn load_single_message(msg_path: &Path, part_dir: &Path) -> Result<Option<ChatMessage>> {
    if !msg_path.is_file() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(msg_path)
        .with_context(|| format!("reading message: {:?}", msg_path))?;

    let info: MessageInfo = serde_json::from_str(&content)
        .with_context(|| format!("parsing message: {:?}", msg_path))?;

    // Load parts for this message
    let parts = load_message_parts(&info.id, part_dir);

    Ok(Some(ChatMessage {
        id: info.id.clone(),
        session_id: info.session_id,
        role: info.role,
        created_at: info.time.created,
        completed_at: info.time.completed,
        parent_id: info.parent_id,
        model_id: info.model_id,
        provider_id: info.provider_id,
        agent: info.agent,
        summary_title: info.summary.and_then(|s| s.title),
        tokens_input: info.tokens.as_ref().and_then(|t| t.input),
        tokens_output: info.tokens.as_ref().and_then(|t| t.output),
        cost: info.cost,
        parts,
    }))
}

/// Get all messages for a session from a specific OpenCode data directory.
pub fn get_session_messages_from_dir(session_id: &str, opencode_dir: &Path) -> Result<Vec<ChatMessage>> {
    let message_dir = opencode_dir.join("storage/message").join(session_id);
    let part_dir = opencode_dir.join("storage/part");

    if !message_dir.exists() {
        tracing::debug!("Message directory does not exist: {:?}", message_dir);
        return Ok(Vec::new());
    }

    let mut messages = Vec::new();

    // Read all message files for this session
    let message_entries = std::fs::read_dir(&message_dir)
        .with_context(|| format!("reading message dir: {:?}", message_dir))?;

    for entry in message_entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let msg_path = entry.path();

        // Only process msg_*.json files
        let is_message_file = msg_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|name| name.starts_with("msg_") && name.ends_with(".json"))
            .unwrap_or(false);

        if !is_message_file || !msg_path.is_file() {
            continue;
        }

        // Read and parse message info
        let content = match std::fs::read_to_string(&msg_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!("Failed to read message file {:?}: {}", msg_path, e);
                continue;
            }
        };

        let info: MessageInfo = match serde_json::from_str(&content) {
            Ok(i) => i,
            Err(e) => {
                tracing::debug!("Failed to parse message file {:?}: {}", msg_path, e);
                continue;
            }
        };

        // Load parts for this message
        let parts = load_message_parts(&info.id, &part_dir);

        messages.push(ChatMessage {
            id: info.id.clone(),
            session_id: info.session_id,
            role: info.role,
            created_at: info.time.created,
            completed_at: info.time.completed,
            parent_id: info.parent_id,
            model_id: info.model_id,
            provider_id: info.provider_id,
            agent: info.agent,
            summary_title: info.summary.and_then(|s| s.title),
            tokens_input: info.tokens.as_ref().and_then(|t| t.input),
            tokens_output: info.tokens.as_ref().and_then(|t| t.output),
            cost: info.cost,
            parts,
        });
    }

    // Sort by created_at ascending (chronological order)
    messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    tracing::debug!(
        "Loaded {} messages for session {} from {:?}",
        messages.len(),
        session_id,
        message_dir
    );

    Ok(messages)
}

/// Load all parts for a specific message.
fn load_message_parts(message_id: &str, part_dir: &Path) -> Vec<ChatMessagePart> {
    let msg_part_dir = part_dir.join(message_id);

    if !msg_part_dir.exists() {
        return Vec::new();
    }

    let mut parts = Vec::new();

    let entries = match std::fs::read_dir(&msg_part_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let part_path = entry.path();

        // Only process prt_*.json files
        let is_part_file = part_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|name| name.starts_with("prt_") && name.ends_with(".json"))
            .unwrap_or(false);

        if !is_part_file || !part_path.is_file() {
            continue;
        }

        let content = match std::fs::read_to_string(&part_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let info: PartInfo = match serde_json::from_str(&content) {
            Ok(i) => i,
            Err(_) => continue,
        };

        // Convert to ChatMessagePart based on type
        let part = match info.part_type.as_str() {
            "text" => ChatMessagePart {
                id: info.id,
                part_type: info.part_type,
                text: info.text,
                text_html: None, // Rendered on-demand via separate endpoint
                tool_name: None,
                tool_input: None,
                tool_output: None,
                tool_status: None,
                tool_title: None,
            },
            "tool" => ChatMessagePart {
                id: info.id,
                part_type: info.part_type,
                text: None,
                text_html: None,
                tool_name: info.tool,
                tool_input: info.state.as_ref().and_then(|s| s.input.clone()),
                tool_output: info.state.as_ref().and_then(|s| s.output.clone()),
                tool_status: info.state.as_ref().and_then(|s| s.status.clone()),
                tool_title: info.state.as_ref().and_then(|s| s.title.clone()),
            },
            // For step-start, step-finish, and other types, include minimal info
            _ => ChatMessagePart {
                id: info.id,
                part_type: info.part_type,
                text: info.text,
                text_html: None,
                tool_name: None,
                tool_input: None,
                tool_output: None,
                tool_status: None,
                tool_title: None,
            },
        };

        parts.push(part);
    }

    // Sort parts by ID (which should be roughly chronological)
    parts.sort_by(|a, b| a.id.cmp(&b.id));

    parts
}

/// Get all messages for a session with pre-rendered markdown HTML.
/// 
/// This is useful for initial load of completed conversations.
/// During streaming, clients should use raw markdown and render client-side.
pub async fn get_session_messages_rendered(session_id: &str) -> Result<Vec<ChatMessage>> {
    let mut messages = get_session_messages_async(session_id).await?;
    
    // Collect all text content that needs rendering
    let texts_to_render: Vec<(usize, usize, String)> = messages.iter().enumerate()
        .flat_map(|(msg_idx, msg)| {
            msg.parts.iter().enumerate()
                .filter(|(_, part)| part.part_type == "text" && part.text.is_some())
                .map(move |(part_idx, part)| {
                    (msg_idx, part_idx, part.text.clone().unwrap())
                })
        })
        .collect();
    
    if texts_to_render.is_empty() {
        return Ok(messages);
    }
    
    // Render all markdown in parallel
    let contents: Vec<String> = texts_to_render.iter()
        .map(|(_, _, text)| text.clone())
        .collect();
    
    let rendered = markdown::render_markdown_batch(contents).await;
    
    // Apply rendered HTML back to messages
    for ((msg_idx, part_idx, _), html) in texts_to_render.into_iter().zip(rendered) {
        messages[msg_idx].parts[part_idx].text_html = Some(html);
    }
    
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_name_from_path() {
        assert_eq!(project_name_from_path("global"), "Global");
        assert_eq!(project_name_from_path(""), "Global");
        assert_eq!(project_name_from_path("/home/wismut/Code/lst"), "lst");
        assert_eq!(project_name_from_path("/home/wismut/byteowlz/kittenx"), "kittenx");
        assert_eq!(project_name_from_path("/home/wismut/byteowlz/govnr"), "govnr");
    }
}
