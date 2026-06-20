//! Repository layer for chat history - handles file and database operations.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json;

use crate::{wordlist, workspace};

use super::models::{
    ChatMessage, ChatMessagePart, ChatSession, MessageInfo, PartInfo, SessionInfo,
};

/// Default legacy (OpenCode) data directory.
pub fn default_legacy_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opencode") // legacy path
}

/// Extract project name from workspace path.
pub fn project_name_from_path(path: &str) -> String {
    if path == "global" || path.is_empty() {
        return "Global".to_string();
    }
    let path_buf = Path::new(path);
    if path_buf.is_dir()
        && let Some(display_name) = workspace::workspace_display_name(path_buf)
    {
        return display_name;
    }
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

// ============================================================================
// Disk repository functions
// ============================================================================

pub fn list_sessions_from_dir(legacy_dir: &Path) -> Result<Vec<ChatSession>> {
    let session_dir = legacy_dir.join("storage/session");

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
            let workspace_path = info
                .directory
                .clone()
                .unwrap_or_else(|| "global".to_string());
            let project_name = project_name_from_path(&workspace_path);
            let is_child = info.parent_id.is_some();

            sessions.push(ChatSession {
                id: info.id.clone(),
                readable_id: wordlist::readable_id_from_session_id(&info.id),
                title: info.title,
                parent_id: info.parent_id,
                workspace_path,
                project_name,
                created_at: info.time.created,
                updated_at: info.time.updated,
                version: info.version,
                is_child,
                source_path: Some(session_path.to_string_lossy().to_string()),
                stats: None,
                model: None,
                provider: None,
            });
        }
    }

    // Sort by updated_at descending (most recent first)
    sessions.sort_by_key(|s| Reverse(s.updated_at));

    tracing::info!("Found {} sessions in {:?}", sessions.len(), session_dir);

    Ok(sessions)
}

/// List sessions grouped by project/workspace.
pub fn list_sessions_grouped() -> Result<HashMap<String, Vec<ChatSession>>> {
    let sessions = list_sessions_from_dir(&default_legacy_data_dir())?;
    let mut grouped: HashMap<String, Vec<ChatSession>> = HashMap::new();

    for session in sessions {
        grouped
            .entry(session.workspace_path.clone())
            .or_default()
            .push(session);
    }

    Ok(grouped)
}

/// Get a single session by ID.
pub fn get_session(session_id: &str) -> Result<Option<ChatSession>> {
    get_session_from_dir(session_id, &default_legacy_data_dir())
}

/// Get a single session by ID from a specific legacy data directory.
pub fn get_session_from_dir(session_id: &str, legacy_dir: &Path) -> Result<Option<ChatSession>> {
    let sessions = list_sessions_from_dir(legacy_dir)?;
    Ok(sessions.into_iter().find(|s| s.id == session_id))
}

/// Update a session's title on disk.
///
/// This reads the session JSON file, updates the title field, and writes it back.
/// Returns the updated session or an error if the session doesn't exist.
pub fn update_session_title(session_id: &str, new_title: &str) -> Result<ChatSession> {
    update_session_title_in_dir(session_id, new_title, &default_legacy_data_dir())
}

/// Update a session's title on disk from a specific legacy data directory.
pub fn update_session_title_in_dir(
    session_id: &str,
    new_title: &str,
    legacy_dir: &Path,
) -> Result<ChatSession> {
    let session_dir = legacy_dir.join("storage/session");

    if !session_dir.exists() {
        anyhow::bail!("Session directory does not exist");
    }

    // Find the session file by iterating through project directories
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

        // Look for the session file in this project directory
        let session_file = project_path.join(format!("{}.json", session_id));
        if !session_file.exists() {
            continue;
        }

        // Found the session file - read, update, and write back
        let content = std::fs::read_to_string(&session_file)
            .with_context(|| format!("reading session file: {:?}", session_file))?;

        let mut info: SessionInfo = serde_json::from_str(&content)
            .with_context(|| format!("parsing session file: {:?}", session_file))?;

        // Update the title and updated timestamp
        info.title = Some(new_title.to_string());
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(info.time.updated);
        info.time.updated = now_ms;

        // Write back
        let updated_content =
            serde_json::to_string_pretty(&info).with_context(|| "serializing updated session")?;
        std::fs::write(&session_file, updated_content)
            .with_context(|| format!("writing session file: {:?}", session_file))?;

        // Return the updated session
        let workspace_path = info
            .directory
            .clone()
            .unwrap_or_else(|| "global".to_string());
        let project_name = project_name_from_path(&workspace_path);
        let is_child = info.parent_id.is_some();

        tracing::info!("Updated session {} title to: {}", session_id, new_title);

        return Ok(ChatSession {
            id: info.id.clone(),
            readable_id: wordlist::readable_id_from_session_id(&info.id),
            title: info.title,
            parent_id: info.parent_id,
            workspace_path,
            project_name,
            created_at: info.time.created,
            updated_at: info.time.updated,
            version: info.version,
            is_child,
            source_path: Some(session_file.to_string_lossy().to_string()),
            stats: None,
            model: None,
            provider: None,
        });
    }

    anyhow::bail!("Session not found: {}", session_id)
}

/// Get all messages for a session using parallel I/O.
pub async fn get_session_messages_parallel(
    session_id: &str,
    legacy_dir: &Path,
) -> Result<Vec<ChatMessage>> {
    let message_dir = legacy_dir.join("storage/message").join(session_id);
    let part_dir = legacy_dir.join("storage/part");

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
    messages.sort_by_key(|a| a.created_at);

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
    let parts = load_message_parts(&info.id, &info.session_id, part_dir);

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
        tokens_reasoning: info.tokens.as_ref().and_then(|t| t.reasoning),
        cost: info.cost,
        client_id: None,
        parts,
    }))
}

/// Get all messages for a session from a specific legacy data directory.
pub fn get_session_messages_from_dir(
    session_id: &str,
    legacy_dir: &Path,
) -> Result<Vec<ChatMessage>> {
    let message_dir = legacy_dir.join("storage/message").join(session_id);
    let part_dir = legacy_dir.join("storage/part");

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
        let parts = load_message_parts(&info.id, &info.session_id, &part_dir);

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
            tokens_reasoning: info.tokens.as_ref().and_then(|t| t.reasoning),
            cost: info.cost,
            client_id: None,
            parts,
        });
    }

    // Sort by created_at ascending (chronological order)
    messages.sort_by_key(|a| a.created_at);

    tracing::debug!(
        "Loaded {} messages for session {} from {:?}",
        messages.len(),
        session_id,
        message_dir
    );

    Ok(messages)
}

/// Load all parts for a specific message.
fn load_message_parts(message_id: &str, session_id: &str, part_dir: &Path) -> Vec<ChatMessagePart> {
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
        if info.message_id != message_id || info.session_id != session_id {
            tracing::debug!(
                "Skipping part {} for mismatched IDs (message={}, session={})",
                info.id,
                info.message_id,
                info.session_id
            );
            continue;
        }

        // Convert to ChatMessagePart based on type
        let part = match info.part_type.as_str() {
            "text" => ChatMessagePart {
                id: info.id,
                part_type: info.part_type,
                text: info.text,
                text_html: None, // Rendered on-demand via separate endpoint
                tool_name: None,
                tool_call_id: None,
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
                tool_call_id: None,
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
                tool_call_id: None,
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

#[cfg(test)]
mod tests {
    use super::*;
}
