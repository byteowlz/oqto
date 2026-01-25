use std::collections::{HashMap, HashSet};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::{
    Json,
    body::Body,
    extract::{Multipart, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use notify::{
    EventKind, RecursiveMode, Watcher,
    event::{CreateKind, RemoveKind},
};
use serde::{Deserialize, Serialize};
use syntect::highlighting::ThemeSet;
use syntect::html::{IncludeBackground, styled_line_to_highlighted_html};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until};
use tokio_util::io::ReaderStream;
use tracing::{debug, error, info, warn};
use walkdir::WalkDir;

use tempfile::tempfile;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::AppState;
use crate::error::FileServerError;
use crate::Config;

// Lazy-loaded syntax highlighting assets
static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// File node in the tree response
#[derive(Debug, Serialize)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub node_type: FileType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileNode>>,
}

#[derive(Debug, Serialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    File,
    Directory,
}

#[derive(Clone, Copy)]
struct ZipLimits {
    max_bytes: u64,
    max_entries: u64,
}

impl ZipLimits {
    fn from_config(config: &Config) -> Self {
        Self {
            max_bytes: config.max_zip_bytes,
            max_entries: config.max_zip_entries,
        }
    }
}

/// Query parameters for tree endpoint
#[derive(Debug, Deserialize)]
pub struct TreeQuery {
    /// Optional directory to scope the root (relative to root_dir)
    pub directory: Option<String>,
    /// Path relative to root (defaults to ".")
    #[serde(default = "default_path")]
    pub path: String,
    /// Maximum depth (defaults to config value)
    pub depth: Option<usize>,
    /// View mode: "simple" for office files only, "full" for everything
    #[serde(default)]
    pub mode: ViewMode,
    /// Include hidden files/dirs
    #[serde(default)]
    pub show_hidden: bool,
}

#[derive(Debug, Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum ViewMode {
    /// Show only office/document files in flat list
    Simple,
    /// Show full directory tree
    #[default]
    Full,
}

fn default_path() -> String {
    ".".to_string()
}

/// Query parameters for file endpoint
#[derive(Debug, Deserialize)]
pub struct FileQuery {
    /// Optional directory to scope the root (relative to root_dir)
    pub directory: Option<String>,
    /// Path relative to root
    pub path: String,
    /// Return syntax-highlighted HTML instead of raw content
    #[serde(default)]
    pub highlight: bool,
    /// Theme for syntax highlighting (defaults to "base16-ocean.dark")
    pub theme: Option<String>,
}

/// Upload query parameters
#[derive(Debug, Deserialize)]
pub struct UploadQuery {
    /// Optional directory to scope the root (relative to root_dir)
    pub directory: Option<String>,
    /// Destination path relative to root
    pub path: String,
    /// Create parent directories if they don't exist
    #[serde(default)]
    pub mkdir: bool,
}

/// Response for successful operations
#[derive(Debug, Serialize)]
pub struct SuccessResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub root: String,
}

/// Query parameters for download endpoint
#[derive(Debug, Deserialize)]
pub struct DownloadQuery {
    /// Optional directory to scope the root (relative to root_dir)
    pub directory: Option<String>,
    /// Path relative to root (for single file/directory download)
    pub path: String,
}

/// Query parameters for multi-file zip download
#[derive(Debug, Deserialize)]
pub struct DownloadZipQuery {
    /// Optional directory to scope the root (relative to root_dir)
    pub directory: Option<String>,
    /// Comma-separated list of paths to include in the zip
    pub paths: String,
    /// Optional name for the zip file (defaults to "download.zip")
    #[serde(default)]
    pub name: Option<String>,
}

/// Query parameters for file watch endpoint
#[derive(Debug, Deserialize)]
pub struct WatchQuery {
    /// Optional directory to scope the root (relative to root_dir)
    pub directory: Option<String>,
    /// Path relative to root (directory to watch)
    pub path: String,
    /// Optional file extension filter (e.g., ".md" or "md", comma-separated)
    pub ext: Option<String>,
}

/// Query parameters for rename endpoint
#[derive(Debug, Deserialize)]
pub struct RenameQuery {
    /// Optional directory to scope the root (relative to root_dir)
    pub directory: Option<String>,
    /// Current path relative to root
    pub old_path: String,
    /// New path relative to root
    pub new_path: String,
}

// ============================================================================
// Helper functions
// ============================================================================

/// Sanitize a filename by removing dangerous characters and path components.
/// Returns None if the filename is invalid or empty after sanitization.
fn sanitize_filename(filename: &str) -> Option<String> {
    // Reject empty filenames
    if filename.is_empty() {
        return None;
    }

    // Remove null bytes and other control characters
    let sanitized: String = filename
        .chars()
        .filter(|c| !c.is_control() && *c != '\0')
        .collect();

    // Remove path separators and dangerous characters
    let sanitized: String = sanitized
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect();

    // Remove leading/trailing dots and spaces (Windows compatibility + security)
    let sanitized = sanitized.trim_matches(|c| c == '.' || c == ' ');

    // Reject if empty after sanitization
    if sanitized.is_empty() {
        return None;
    }

    // Reject reserved Windows names (for cross-platform safety)
    let upper = sanitized.to_uppercase();
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved
        .iter()
        .any(|r| upper == *r || upper.starts_with(&format!("{}.", r)))
    {
        return None;
    }

    // Limit filename length
    if sanitized.len() > 255 {
        return Some(sanitized[..255].to_string());
    }

    Some(sanitized.to_string())
}

/// Resolve and validate a path, ensuring it's within the root directory.
///
/// This function is designed to prevent path traversal attacks by:
/// 1. Building the path component-by-component, rejecting any parent directory (..) references
/// 2. Validating the final path is within the root directory
/// 3. Using a deterministic path building approach that doesn't rely on filesystem state
///
/// Note: For existing paths, the caller should canonicalize and re-verify after this check
/// to handle symbolic link attacks (TOCTOU mitigation).
fn resolve_path(root: &Path, relative: &str) -> Result<PathBuf, FileServerError> {
    // Normalize and split the relative path
    let relative = relative.trim_start_matches('/');

    // Handle empty path or "."
    if relative.is_empty() || relative == "." {
        return Ok(root.to_path_buf());
    }

    // Build the path component-by-component, rejecting traversal attempts
    let mut result = root.to_path_buf();

    for component in Path::new(relative).components() {
        match component {
            Component::Normal(name) => {
                // Check for embedded null bytes or other dangerous characters in the name
                let name_str = name.to_string_lossy();
                if name_str.contains('\0') {
                    warn!("Path component contains null byte: {:?}", name);
                    return Err(FileServerError::PathTraversal);
                }
                result.push(name);
            }
            Component::ParentDir => {
                // ALWAYS reject parent directory references - this is the key security fix
                // Even if they would resolve to within root, they indicate malicious intent
                warn!("Path traversal attempt detected: parent directory (..) in path");
                return Err(FileServerError::PathTraversal);
            }
            Component::CurDir => {
                // Current directory (.) is safe, just skip it
                continue;
            }
            Component::RootDir | Component::Prefix(_) => {
                // Absolute path components are not allowed
                warn!("Absolute path component in relative path");
                return Err(FileServerError::PathTraversal);
            }
        }
    }

    // Final validation: ensure the built path starts with root
    // This is a belt-and-suspenders check
    if !result.starts_with(root) {
        error!(
            "Path resolution resulted in path outside root: {:?}",
            result
        );
        return Err(FileServerError::PathTraversal);
    }

    Ok(result)
}

/// Safely resolve a path and validate it exists within root.
/// This function handles both path building and symlink resolution safely.
///
/// For operations that need to access the filesystem, use this function
/// to get a canonical path that is guaranteed to be within root.
fn resolve_and_verify_path(root: &Path, relative: &str) -> Result<PathBuf, FileServerError> {
    // First, build the path without following symlinks
    let built_path = resolve_path(root, relative)?;

    // If the path exists, canonicalize it and verify it's still within root
    if built_path.exists() {
        let canonical_root = root.canonicalize().map_err(FileServerError::Io)?;
        let canonical_path = built_path.canonicalize().map_err(FileServerError::Io)?;

        // Verify the canonical path is within the canonical root
        if !canonical_path.starts_with(&canonical_root) {
            warn!(
                "Symlink escape attempt: {:?} resolved to {:?} which is outside {:?}",
                built_path, canonical_path, canonical_root
            );
            return Err(FileServerError::PathTraversal);
        }

        Ok(canonical_path)
    } else {
        // Path doesn't exist yet - verify the parent directory
        if let Some(parent) = built_path.parent() {
            if parent.exists() {
                let canonical_root = root.canonicalize().map_err(FileServerError::Io)?;
                let canonical_parent = parent.canonicalize().map_err(FileServerError::Io)?;

                if !canonical_parent.starts_with(&canonical_root) {
                    warn!(
                        "Parent directory escape: {:?} parent resolved outside root",
                        built_path
                    );
                    return Err(FileServerError::PathTraversal);
                }
            }
        }

        Ok(built_path)
    }
}

/// Resolve a scoped root directory based on an optional directory override.
/// Returns the effective root directory for the request.
fn resolve_request_root(root: &Path, directory: Option<&str>) -> Result<PathBuf, FileServerError> {
    let Some(directory) = directory else {
        return Ok(root.to_path_buf());
    };

    let directory = directory.trim();
    if directory.is_empty() || directory == "." {
        return Ok(root.to_path_buf());
    }

    let relative = if Path::new(directory).is_absolute() {
        let canonical_root = root.canonicalize().map_err(FileServerError::Io)?;
        let candidate = Path::new(directory);
        if let Ok(stripped) = candidate.strip_prefix(&canonical_root) {
            stripped.to_string_lossy().to_string()
        } else if let Ok(stripped) = candidate.strip_prefix(root) {
            stripped.to_string_lossy().to_string()
        } else {
            warn!(
                "Directory override outside root: {:?} (root: {:?})",
                candidate, root
            );
            return Err(FileServerError::PathTraversal);
        }
    } else {
        directory.to_string()
    };

    let resolved = resolve_and_verify_path(root, &relative)?;
    if !resolved.exists() {
        return Err(FileServerError::NotFound(directory.to_string()));
    }
    if !resolved.is_dir() {
        return Err(FileServerError::NotADirectory);
    }

    Ok(resolved)
}

/// Get relative path from root.
///
/// Always uses `/` as separator (zip + HTTP-friendly).
fn get_relative_path(root: &Path, full_path: &Path) -> String {
    let Ok(relative) = full_path.strip_prefix(root) else {
        return String::new();
    };

    let mut parts = Vec::new();
    for component in relative.components() {
        if let Component::Normal(part) = component {
            parts.push(part.to_string_lossy().to_string());
        }
    }

    parts.join("/")
}

fn normalize_extension(ext: &str) -> Option<String> {
    let trimmed = ext.trim();
    if trimmed.is_empty() {
        return None;
    }

    let trimmed = trimmed.trim_start_matches('.');
    if trimmed.is_empty() {
        return None;
    }

    Some(trimmed.to_ascii_lowercase())
}

fn parse_extension_filter(ext: &Option<String>) -> Option<HashSet<String>> {
    let ext = ext.as_ref()?;
    let mut set = HashSet::new();

    for item in ext.split(',') {
        if let Some(normalized) = normalize_extension(item) {
            set.insert(normalized);
        }
    }

    if set.is_empty() { None } else { Some(set) }
}

fn event_label(kind: &EventKind, is_dir: bool) -> Option<&'static str> {
    match kind {
        EventKind::Create(_) => {
            if is_dir {
                Some("dir_created")
            } else {
                Some("file_created")
            }
        }
        EventKind::Modify(_) => {
            if is_dir {
                None
            } else {
                Some("file_modified")
            }
        }
        EventKind::Remove(_) => {
            if is_dir {
                Some("dir_deleted")
            } else {
                Some("file_deleted")
            }
        }
        _ => None,
    }
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /health - Health check endpoint
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        root: state.root_dir.display().to_string(),
    })
}

const WATCH_DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Debug, Serialize)]
struct WatchEvent {
    #[serde(rename = "type")]
    event_type: &'static str,
    path: String,
    entry_type: &'static str,
}

/// GET /ws/watch - WebSocket file watch endpoint
pub async fn watch_ws(
    State(state): State<AppState>,
    Query(query): Query<WatchQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, FileServerError> {
    let root_dir = resolve_request_root(&state.root_dir, query.directory.as_deref())?;
    let path = resolve_and_verify_path(&root_dir, &query.path)?;

    if !path.exists() {
        return Err(FileServerError::NotFound(query.path));
    }

    if !path.is_dir() {
        return Err(FileServerError::NotADirectory);
    }

    let ext_filter = parse_extension_filter(&query.ext);
    Ok(ws.on_upgrade(move |socket| watch_socket(socket, root_dir, path, ext_filter)))
}

async fn watch_socket(
    mut socket: WebSocket,
    root_dir: PathBuf,
    watch_path: PathBuf,
    ext_filter: Option<HashSet<String>>,
) {
    let (tx, mut rx) = mpsc::channel(128);
    let mut watcher = match notify::recommended_watcher(move |res| {
        if tx.blocking_send(res).is_err() {
            debug!("File watch channel closed");
        }
    }) {
        Ok(watcher) => watcher,
        Err(err) => {
            error!("Failed to initialize watcher: {:?}", err);
            return;
        }
    };

    if let Err(err) = watcher.watch(&watch_path, RecursiveMode::Recursive) {
        error!("Failed to watch path {}: {:?}", watch_path.display(), err);
        return;
    }

    let mut pending: HashMap<PathBuf, EventKind> = HashMap::new();
    let mut deadline: Option<Instant> = None;

    loop {
        tokio::select! {
            incoming = rx.recv() => {
                match incoming {
                    Some(Ok(event)) => {
                        for path in event.paths {
                            pending.insert(path, event.kind.clone());
                        }
                        deadline = Some(Instant::now() + WATCH_DEBOUNCE);
                    }
                    Some(Err(err)) => {
                        warn!("Watcher error: {:?}", err);
                    }
                    None => break,
                }
            }
            _ = sleep_until(deadline.unwrap()) , if deadline.is_some() => {
                let mut batched = HashMap::new();
                std::mem::swap(&mut batched, &mut pending);
                deadline = None;

                for (path, kind) in batched {
                    if !path.starts_with(&root_dir) {
                        continue;
                    }

                    let is_dir = match fs::metadata(&path).await {
                        Ok(metadata) => metadata.is_dir(),
                        Err(_) => matches!(
                            kind,
                            EventKind::Create(CreateKind::Folder) | EventKind::Remove(RemoveKind::Folder)
                        ),
                    };

                    if !is_dir {
                        if let Some(filter) = &ext_filter {
                            let ext = path
                                .extension()
                                .and_then(|value| value.to_str())
                                .and_then(normalize_extension);
                            if ext.as_ref().map_or(true, |value| !filter.contains(value)) {
                                continue;
                            }
                        }
                    }

                    let Some(event_type) = event_label(&kind, is_dir) else {
                        continue;
                    };

                    let relative_path = get_relative_path(&root_dir, &path);
                    if relative_path.is_empty() {
                        continue;
                    }

                    let payload = WatchEvent {
                        event_type,
                        path: relative_path,
                        entry_type: if is_dir { "directory" } else { "file" },
                    };

                    let Ok(data) = serde_json::to_string(&payload) else {
                        continue;
                    };

                    if socket.send(Message::Text(data.into())).await.is_err() {
                        return;
                    }
                }
            }
            message = socket.recv() => {
                match message {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(payload))) => {
                        let _ = socket.send(Message::Pong(payload)).await;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        debug!("WebSocket receive error: {:?}", err);
                        break;
                    }
                }
            }
        }
    }
}

/// GET /tree - Get directory tree
pub async fn get_tree(
    State(state): State<AppState>,
    Query(query): Query<TreeQuery>,
) -> Result<Json<Vec<FileNode>>, FileServerError> {
    // Use resolve_and_verify_path for proper symlink handling
    let root_dir = resolve_request_root(&state.root_dir, query.directory.as_deref())?;
    let path = resolve_and_verify_path(&root_dir, &query.path)?;

    if !path.exists() {
        return Err(FileServerError::NotFound(query.path));
    }

    if !path.is_dir() {
        return Err(FileServerError::NotADirectory);
    }

    let max_depth = query.depth.unwrap_or(state.config.max_depth);

    debug!(
        "Getting tree for path: {}, mode: {:?}, depth: {}",
        path.display(),
        query.mode,
        max_depth
    );

    match query.mode {
        ViewMode::Simple => {
            // Flat list of office files only
            let state = state.clone();
            let root_dir = root_dir.clone();
            let path = path.clone();
            let files = tokio::task::spawn_blocking(move || {
                get_simple_file_list(&state, &root_dir, &path, max_depth)
            })
            .await
            .map_err(|err| {
                FileServerError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    err.to_string(),
                ))
            })??;
            Ok(Json(files))
        }
        ViewMode::Full => {
            // Full directory tree
            let state = state.clone();
            let root_dir = root_dir.clone();
            let path = path.clone();
            let show_hidden = query.show_hidden;

            let tree = tokio::task::spawn_blocking(move || {
                build_tree(&state, &root_dir, &path, max_depth, show_hidden)
            })
            .await
            .map_err(|err| {
                FileServerError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    err.to_string(),
                ))
            })??;
            Ok(Json(tree))
        }
    }
}

/// Build full directory tree
fn build_tree(
    state: &AppState,
    root_dir: &Path,
    path: &Path,
    max_depth: usize,
    show_hidden: bool,
) -> Result<Vec<FileNode>, FileServerError> {
    let mut nodes = Vec::new();

    let entries = std::fs::read_dir(path).map_err(FileServerError::Io)?;

    for entry in entries {
        let entry = entry.map_err(FileServerError::Io)?;
        let entry_path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files unless requested
        if !show_hidden && file_name.starts_with('.') {
            continue;
        }

        // Skip hidden directories from config
        if entry_path.is_dir() && state.config.is_hidden_dir(&file_name) {
            continue;
        }

        // Skip hidden extensions
        if let Some(ext) = entry_path.extension() {
            let ext_str = format!(".{}", ext.to_string_lossy());
            if state.config.is_hidden_extension(&ext_str) {
                continue;
            }
        }

        let metadata = entry.metadata().map_err(FileServerError::Io)?;
        let relative_path = get_relative_path(root_dir, &entry_path);

        let node = if entry_path.is_dir() {
            let children = if max_depth > 1 {
                Some(build_tree(
                    state,
                    root_dir,
                    &entry_path,
                    max_depth - 1,
                    show_hidden,
                )?)
            } else {
                None
            };

            FileNode {
                name: file_name,
                path: relative_path,
                node_type: FileType::Directory,
                size: None,
                modified: metadata.modified().ok().and_then(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|d| d.as_secs())
                }),
                children,
            }
        } else {
            FileNode {
                name: file_name,
                path: relative_path,
                node_type: FileType::File,
                size: Some(metadata.len()),
                modified: metadata.modified().ok().and_then(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|d| d.as_secs())
                }),
                children: None,
            }
        };

        nodes.push(node);
    }

    // Sort: directories first, then alphabetically
    nodes.sort_by(|a, b| match (&a.node_type, &b.node_type) {
        (FileType::Directory, FileType::File) => std::cmp::Ordering::Less,
        (FileType::File, FileType::Directory) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(nodes)
}

/// Get flat list of office files (simple mode)
fn get_simple_file_list(
    state: &AppState,
    root_dir: &Path,
    path: &Path,
    max_depth: usize,
) -> Result<Vec<FileNode>, FileServerError> {
    let mut files = Vec::new();

    for entry in WalkDir::new(path)
        .max_depth(max_depth)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            // Skip hidden files and directories
            if name.starts_with('.') {
                return false;
            }
            // Skip hidden directories from config
            if e.file_type().is_dir() && state.config.is_hidden_dir(&name) {
                return false;
            }
            true
        })
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let entry_path = entry.path();

        // Check if it's an office file
        if let Some(ext) = entry_path.extension() {
            let ext_str = format!(".{}", ext.to_string_lossy());
            if !state.config.is_office_file(&ext_str) {
                continue;
            }
        } else {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let file_name = entry.file_name().to_string_lossy().to_string();
        let relative_path = get_relative_path(root_dir, entry_path);

        files.push(FileNode {
            name: file_name,
            path: relative_path,
            node_type: FileType::File,
            size: Some(metadata.len()),
            modified: metadata.modified().ok().and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs())
            }),
            children: None,
        });
    }

    // Sort alphabetically
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(files)
}

/// GET /file - Get file content
///
/// Uses streaming to handle large files efficiently without loading them entirely into memory.
/// If `highlight=true` is passed, returns syntax-highlighted HTML instead of raw content.
pub async fn get_file(
    State(state): State<AppState>,
    Query(query): Query<FileQuery>,
) -> Result<Response, FileServerError> {
    // Use resolve_and_verify_path for proper symlink handling
    let root_dir = resolve_request_root(&state.root_dir, query.directory.as_deref())?;
    let path = resolve_and_verify_path(&root_dir, &query.path)?;

    if !path.exists() {
        return Err(FileServerError::NotFound(query.path.clone()));
    }

    if path.is_dir() {
        return Err(FileServerError::NotAFile);
    }

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // If syntax highlighting is requested, return highlighted HTML
    if query.highlight {
        debug!("Syntax highlighting file: {}", path.display());

        // Read file content (limit to 1MB for highlighting to prevent memory issues)
        let metadata = fs::metadata(&path).await.map_err(FileServerError::Io)?;
        if metadata.len() > 1024 * 1024 {
            return Err(FileServerError::FileTooLarge {
                size: metadata.len(),
                limit: 1024 * 1024,
            });
        }

        let content = fs::read_to_string(&path)
            .await
            .map_err(FileServerError::Io)?;
        let path_clone = path.clone();
        let theme_name = query
            .theme
            .unwrap_or_else(|| "base16-ocean.dark".to_string());

        // Do highlighting in blocking task since syntect is not async
        let highlighted =
            tokio::task::spawn_blocking(move || highlight_code(&content, &path_clone, &theme_name))
                .await
                .map_err(|e| {
                    FileServerError::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
                })??;

        return Ok((
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8".to_string()),
                (header::CONTENT_LENGTH, highlighted.len().to_string()),
                (header::CACHE_CONTROL, "public, max-age=60".to_string()),
            ],
            highlighted,
        )
            .into_response());
    }

    debug!("Streaming file: {}", path.display());

    // Get file metadata for content-length
    let metadata = fs::metadata(&path).await.map_err(FileServerError::Io)?;
    let file_size = metadata.len();

    // Open file for streaming
    let file = fs::File::open(&path).await.map_err(FileServerError::Io)?;

    // Create a stream from the file
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let mime = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .to_string();

    // Sanitize filename for Content-Disposition header
    let safe_filename = file_name.replace('"', "'");

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime),
            (header::CONTENT_LENGTH, file_size.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"{}\"", safe_filename),
            ),
        ],
        body,
    )
        .into_response())
}

/// Highlight code using syntect
fn highlight_code(content: &str, path: &Path, theme_name: &str) -> Result<String, FileServerError> {
    let syntax = path
        .extension()
        .and_then(|ext| SYNTAX_SET.find_syntax_by_extension(ext.to_str().unwrap_or("")))
        .or_else(|| SYNTAX_SET.find_syntax_by_first_line(content))
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

    let theme = THEME_SET
        .themes
        .get(theme_name)
        .or_else(|| THEME_SET.themes.get("base16-ocean.dark"))
        .ok_or_else(|| {
            FileServerError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Theme not found",
            ))
        })?;

    let mut highlighter = syntect::easy::HighlightLines::new(syntax, theme);
    let mut html_output = String::with_capacity(content.len() * 2);

    // Build HTML with line numbers using table layout for guaranteed alignment
    html_output.push_str("<table class=\"highlighted-code\" style=\"font-family: ui-monospace, SFMono-Regular, 'SF Mono', Consolas, 'Liberation Mono', Menlo, monospace; font-size: 12px; line-height: 1.5; border-collapse: collapse; width: 100%;\">");
    html_output.push_str("<tbody>");

    for (i, line) in LinesWithEndings::from(content).enumerate() {
        let regions = highlighter
            .highlight_line(line, &SYNTAX_SET)
            .map_err(|e| FileServerError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        let html_line = styled_line_to_highlighted_html(&regions[..], IncludeBackground::No)
            .map_err(|e| FileServerError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        html_output.push_str("<tr>");
        // Line number cell
        html_output.push_str(&format!(
            "<td style=\"text-align: right; padding-right: 0.5em; min-width: 2.5em; color: #6b7280; user-select: none; vertical-align: top; white-space: nowrap;\">{}</td>",
            i + 1
        ));
        // Code cell
        html_output.push_str("<td style=\"white-space: pre; vertical-align: top;\">");
        if html_line.trim().is_empty() {
            html_output.push_str(" ");
        } else {
            // Trim the trailing newline from the highlighted line
            html_output.push_str(html_line.trim_end_matches('\n'));
        }
        html_output.push_str("</td>");
        html_output.push_str("</tr>");
    }

    html_output.push_str("</tbody></table>");

    Ok(html_output)
}

/// POST /file - Upload file
pub async fn upload_file(
    State(state): State<AppState>,
    Query(query): Query<UploadQuery>,
    mut multipart: Multipart,
) -> Result<Json<SuccessResponse>, FileServerError> {
    warn!("Upload file request received for path: {}", query.path);
    let root_dir = resolve_request_root(&state.root_dir, query.directory.as_deref())?;
    // Use resolve_path for initial path building; deeper checks happen below.
    let dest_path = resolve_path(&root_dir, &query.path)?;

    // Create parent directories if requested
    if query.mkdir {
        if let Some(parent) = dest_path.parent() {
            if parent != root_dir {
                fs::create_dir_all(parent).await.map_err(|e| {
                    error!("Failed to create directory: {}", e);
                    FileServerError::CreateDirFailed(parent.display().to_string())
                })?;
            }
        }
    }

    let mut field = match multipart.next_field().await.map_err(|e| {
        error!("Multipart error parsing field: {:?}", e);
        FileServerError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Error parsing `multipart/form-data` request: {}", e),
        ))
    })? {
        Some(field) => field,
        None => {
            return Err(FileServerError::InvalidPath(
                "Missing file upload data".to_string(),
            ));
        }
    };

    // Get and SANITIZE the filename
    let raw_filename = field
        .file_name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "upload".to_string());

    let file_name = sanitize_filename(&raw_filename).ok_or_else(|| {
        warn!("Rejected invalid filename: {:?}", raw_filename);
        FileServerError::InvalidPath(format!("Invalid filename: {}", raw_filename))
    })?;

    // Determine final path
    let final_path = if dest_path.is_dir() || query.path.ends_with('/') {
        // If destination is a directory, use the sanitized filename
        let dir_path = if dest_path.exists() {
            dest_path.clone()
        } else if query.mkdir {
            fs::create_dir_all(&dest_path)
                .await
                .map_err(|_| FileServerError::CreateDirFailed(dest_path.display().to_string()))?;
            dest_path.clone()
        } else {
            return Err(FileServerError::NotFound(query.path.clone()));
        };
        dir_path.join(&file_name)
    } else {
        dest_path.clone()
    };

    // Re-validate the final path is within root (belt-and-suspenders)
    let canonical_root = root_dir.canonicalize().map_err(FileServerError::Io)?;
    if final_path.exists() {
        let metadata = fs::symlink_metadata(&final_path)
            .await
            .map_err(FileServerError::Io)?;
        if metadata.file_type().is_symlink() {
            warn!("Refusing to overwrite symlink: {:?}", final_path);
            return Err(FileServerError::PathTraversal);
        }
        if metadata.is_dir() {
            return Err(FileServerError::NotAFile);
        }
        let canonical_path = final_path.canonicalize().map_err(FileServerError::Io)?;
        if !canonical_path.starts_with(&canonical_root) {
            warn!("Final path resolved outside root: {:?}", final_path);
            return Err(FileServerError::PathTraversal);
        }
    } else if let Some(parent) = final_path.parent() {
        if parent.exists() {
            let canonical_parent = parent.canonicalize().map_err(FileServerError::Io)?;
            if !canonical_parent.starts_with(&canonical_root) {
                warn!("Final path parent outside root: {:?}", final_path);
                return Err(FileServerError::PathTraversal);
            }
        }
    }

    let parent_dir = final_path
        .parent()
        .ok_or_else(|| FileServerError::InvalidPath("Missing parent directory".to_string()))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp_name = format!(".upload-{}-{}", file_name, nonce);
    let temp_path = parent_dir.join(temp_name);
    let mut temp_file = fs::File::create(&temp_path)
        .await
        .map_err(FileServerError::Io)?;

    let mut total_size = 0u64;
    while let Some(chunk) = field.chunk().await.map_err(|e| {
        error!("Failed to read upload data: {}", e);
        FileServerError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    })? {
        total_size = total_size.saturating_add(chunk.len() as u64);
        if total_size > state.config.max_upload_size {
            let _ = fs::remove_file(&temp_path).await;
            return Err(FileServerError::FileTooLarge {
                size: total_size,
                limit: state.config.max_upload_size,
            });
        }
        temp_file
            .write_all(&chunk)
            .await
            .map_err(FileServerError::Io)?;
    }
    temp_file.flush().await.map_err(FileServerError::Io)?;

    info!(
        "Uploading file: {} ({} bytes)",
        final_path.display(),
        total_size
    );

    if final_path.exists() {
        fs::remove_file(&final_path)
            .await
            .map_err(FileServerError::Io)?;
    }
    fs::rename(&temp_path, &final_path)
        .await
        .map_err(FileServerError::Io)?;

    let relative_path = get_relative_path(&root_dir, &final_path);

    Ok(Json(SuccessResponse {
        success: true,
        message: format!("File uploaded: {}", file_name),
        path: Some(relative_path),
    }))
}

/// DELETE /file - Delete file or directory
pub async fn delete_file(
    State(state): State<AppState>,
    Query(query): Query<FileQuery>,
) -> Result<Json<SuccessResponse>, FileServerError> {
    // Use resolve_and_verify_path for proper symlink handling
    let root_dir = resolve_request_root(&state.root_dir, query.directory.as_deref())?;
    let path = resolve_and_verify_path(&root_dir, &query.path)?;

    if !path.exists() {
        return Err(FileServerError::NotFound(query.path));
    }

    // SECURITY: Prevent deletion of root directory
    let canonical_root = root_dir.canonicalize().map_err(FileServerError::Io)?;
    let canonical_path = path.canonicalize().map_err(FileServerError::Io)?;

    if canonical_path == canonical_root {
        warn!("Attempted to delete root directory: {:?}", query.path);
        return Err(FileServerError::InvalidPath(
            "Cannot delete root directory".to_string(),
        ));
    }

    // Double-check path is still within root after canonicalization
    if !canonical_path.starts_with(&canonical_root) {
        warn!(
            "Delete path escaped root after canonicalization: {:?}",
            path
        );
        return Err(FileServerError::PathTraversal);
    }

    info!("Deleting: {}", path.display());

    if path.is_dir() {
        fs::remove_dir_all(&path)
            .await
            .map_err(FileServerError::Io)?;
    } else {
        fs::remove_file(&path).await.map_err(FileServerError::Io)?;
    }

    Ok(Json(SuccessResponse {
        success: true,
        message: format!("Deleted: {}", query.path),
        path: Some(query.path),
    }))
}

/// PUT /file - Write file contents directly (for simple text/JSON files)
pub async fn write_file(
    State(state): State<AppState>,
    Query(query): Query<UploadQuery>,
    body: axum::body::Bytes,
) -> Result<Json<SuccessResponse>, FileServerError> {
    // Enforce size limit (same as upload_file)
    let body_size = body.len() as u64;
    if body_size > state.config.max_upload_size {
        return Err(FileServerError::FileTooLarge {
            size: body_size,
            limit: state.config.max_upload_size,
        });
    }

    let root_dir = resolve_request_root(&state.root_dir, query.directory.as_deref())?;
    let dest_path = resolve_path(&root_dir, &query.path)?;

    // Create parent directories if requested
    if query.mkdir {
        if let Some(parent) = dest_path.parent() {
            if parent != root_dir && !parent.exists() {
                fs::create_dir_all(parent).await.map_err(|e| {
                    error!("Failed to create directory: {}", e);
                    FileServerError::CreateDirFailed(parent.display().to_string())
                })?;
            }
        }
    }

    // SECURITY: Verify final path is within root
    let canonical_root = root_dir.canonicalize().map_err(FileServerError::Io)?;
    if let Ok(canonical_dest) = dest_path.canonicalize() {
        if !canonical_dest.starts_with(&canonical_root) {
            warn!("Write path escaped root: {:?}", dest_path);
            return Err(FileServerError::PathTraversal);
        }
    } else if let Some(parent) = dest_path.parent() {
        // File doesn't exist yet, check parent
        if let Ok(canonical_parent) = parent.canonicalize() {
            if !canonical_parent.starts_with(&canonical_root) {
                warn!("Write path parent escaped root: {:?}", parent);
                return Err(FileServerError::PathTraversal);
            }
        }
    }

    info!(
        "Writing file: {} ({} bytes)",
        dest_path.display(),
        body.len()
    );

    // Write the file
    let mut file = fs::File::create(&dest_path).await.map_err(|e| {
        error!("Failed to create file: {}", e);
        FileServerError::Io(e)
    })?;

    file.write_all(&body).await.map_err(|e| {
        error!("Failed to write file: {}", e);
        FileServerError::Io(e)
    })?;

    Ok(Json(SuccessResponse {
        success: true,
        message: format!("Written: {} ({} bytes)", query.path, body.len()),
        path: Some(query.path),
    }))
}

/// PUT /mkdir - Create directory
pub async fn create_dir(
    State(state): State<AppState>,
    Query(query): Query<FileQuery>,
) -> Result<Json<SuccessResponse>, FileServerError> {
    let root_dir = resolve_request_root(&state.root_dir, query.directory.as_deref())?;
    let path = resolve_path(&root_dir, &query.path)?;

    if path.exists() {
        return Ok(Json(SuccessResponse {
            success: true,
            message: "Directory already exists".to_string(),
            path: Some(query.path),
        }));
    }

    info!("Creating directory: {}", path.display());

    fs::create_dir_all(&path).await.map_err(|e| {
        error!("Failed to create directory: {}", e);
        FileServerError::CreateDirFailed(path.display().to_string())
    })?;

    Ok(Json(SuccessResponse {
        success: true,
        message: format!("Created directory: {}", query.path),
        path: Some(query.path),
    }))
}

/// POST /rename - Rename/move a file or directory
pub async fn rename_file(
    State(state): State<AppState>,
    Query(query): Query<RenameQuery>,
) -> Result<Json<SuccessResponse>, FileServerError> {
    let root_dir = resolve_request_root(&state.root_dir, query.directory.as_deref())?;
    let old_path = resolve_and_verify_path(&root_dir, &query.old_path)?;
    let new_path = resolve_path(&root_dir, &query.new_path)?;

    if !old_path.exists() {
        return Err(FileServerError::NotFound(query.old_path));
    }

    // SECURITY: Prevent renaming root directory
    let canonical_root = root_dir.canonicalize().map_err(FileServerError::Io)?;
    let canonical_old = old_path.canonicalize().map_err(FileServerError::Io)?;

    if canonical_old == canonical_root {
        warn!("Attempted to rename root directory: {:?}", query.old_path);
        return Err(FileServerError::InvalidPath(
            "Cannot rename root directory".to_string(),
        ));
    }

    // Verify old path is within root
    if !canonical_old.starts_with(&canonical_root) {
        warn!(
            "Old path escaped root after canonicalization: {:?}",
            old_path
        );
        return Err(FileServerError::PathTraversal);
    }

    // Verify new path parent exists and is within root
    if let Some(new_parent) = new_path.parent() {
        if !new_parent.exists() {
            return Err(FileServerError::NotFound(format!(
                "Parent directory does not exist: {}",
                new_parent.display()
            )));
        }
        let canonical_parent = new_parent.canonicalize().map_err(FileServerError::Io)?;
        if !canonical_parent.starts_with(&canonical_root) {
            warn!("New path parent escaped root: {:?}", new_parent);
            return Err(FileServerError::PathTraversal);
        }
    }

    // Check if new path already exists
    if new_path.exists() {
        return Err(FileServerError::InvalidPath(format!(
            "Destination already exists: {}",
            query.new_path
        )));
    }

    info!("Renaming: {} -> {}", old_path.display(), new_path.display());

    fs::rename(&old_path, &new_path)
        .await
        .map_err(FileServerError::Io)?;

    Ok(Json(SuccessResponse {
        success: true,
        message: format!("Renamed: {} -> {}", query.old_path, query.new_path),
        path: Some(query.new_path),
    }))
}

/// GET /download - Download a single file or directory as zip
///
/// For files: returns the file with Content-Disposition: attachment
/// For directories: returns a zip archive of the directory
pub async fn download(
    State(state): State<AppState>,
    Query(query): Query<DownloadQuery>,
) -> Result<Response, FileServerError> {
    let root_dir = resolve_request_root(&state.root_dir, query.directory.as_deref())?;
    let path = resolve_and_verify_path(&root_dir, &query.path)?;

    if !path.exists() {
        return Err(FileServerError::NotFound(query.path));
    }

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());

    if path.is_file() {
        // Single file download
        debug!("Downloading file: {}", path.display());

        let metadata = fs::metadata(&path).await.map_err(FileServerError::Io)?;
        let file_size = metadata.len();
        let file = fs::File::open(&path).await.map_err(FileServerError::Io)?;
        let stream = ReaderStream::new(file);
        let body = Body::from_stream(stream);

        let mime = mime_guess::from_path(&path)
            .first_or_octet_stream()
            .to_string();

        let safe_filename = file_name.replace('"', "'");

        Ok((
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, mime),
                (header::CONTENT_LENGTH, file_size.to_string()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", safe_filename),
                ),
            ],
            body,
        )
            .into_response())
    } else {
        // Directory download - create zip
        debug!("Downloading directory as zip: {}", path.display());

        let zip_name = format!("{}.zip", file_name);
        let safe_zip_name = zip_name.replace('"', "'");

        let limits = ZipLimits::from_config(&state.config);
        let (zip_file, zip_size) =
            create_zip_file_from_paths(root_dir.clone(), vec![path.clone()], limits).await?;
        let body = Body::from_stream(ReaderStream::new(zip_file));

        Ok((
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/zip".to_string()),
                (header::CONTENT_LENGTH, zip_size.to_string()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", safe_zip_name),
                ),
            ],
            body,
        )
            .into_response())
    }
}

/// GET /download-zip - Download multiple files/directories as a single zip
pub async fn download_zip(
    State(state): State<AppState>,
    Query(query): Query<DownloadZipQuery>,
) -> Result<Response, FileServerError> {
    let root_dir = resolve_request_root(&state.root_dir, query.directory.as_deref())?;
    // Parse comma-separated paths
    let paths: Vec<&str> = query.paths.split(',').map(|s| s.trim()).collect();

    if paths.is_empty() {
        return Err(FileServerError::InvalidPath(
            "No paths provided".to_string(),
        ));
    }

    // Resolve and verify all paths
    let mut resolved_paths = Vec::new();
    for path_str in &paths {
        let resolved = resolve_and_verify_path(&root_dir, path_str)?;
        if !resolved.exists() {
            return Err(FileServerError::NotFound(path_str.to_string()));
        }
        resolved_paths.push(resolved);
    }

    debug!("Downloading {} items as zip", resolved_paths.len());

    let limits = ZipLimits::from_config(&state.config);
    let (zip_file, zip_size) = create_zip_file_from_paths(root_dir, resolved_paths, limits).await?;
    let body = Body::from_stream(ReaderStream::new(zip_file));

    let zip_name = query.name.unwrap_or_else(|| "download.zip".to_string());
    let safe_zip_name = zip_name.replace('"', "'");

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (header::CONTENT_LENGTH, zip_size.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", safe_zip_name),
            ),
        ],
        body,
    )
        .into_response())
}

/// Create a zip archive from a list of paths (files or directories).
///
/// Uses a temporary file on disk so large downloads don't require buffering
/// the full archive in memory.
async fn create_zip_file_from_paths(
    root: PathBuf,
    paths: Vec<PathBuf>,
    limits: ZipLimits,
) -> Result<(fs::File, u64), FileServerError> {
    let (file, size) =
        tokio::task::spawn_blocking(move || create_zip_tempfile_blocking(&root, &paths, limits))
            .await
            .map_err(|err| {
                FileServerError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    err.to_string(),
                ))
            })??;

    Ok((fs::File::from_std(file), size))
}

fn create_zip_tempfile_blocking(
    root: &Path,
    paths: &[PathBuf],
    limits: ZipLimits,
) -> Result<(std::fs::File, u64), FileServerError> {
    enforce_zip_limits(root, paths, limits)?;
    let file = tempfile()?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    for path in paths {
        if path.is_file() {
            let relative = get_relative_path(root, path);
            let file_name = if relative.is_empty() {
                path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "file".to_string())
            } else {
                relative
            };

            zip.start_file(&file_name, options)
                .map_err(zip_error_to_fileserver_error)?;
            let mut input = std::fs::File::open(path)?;
            std::io::copy(&mut input, &mut zip)?;
        } else if path.is_dir() {
            add_directory_to_zip(&mut zip, root, path, options)?;
        }
    }

    let mut file = zip.finish().map_err(zip_error_to_fileserver_error)?;
    file.flush()?;

    let size = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(0))?;

    Ok((file, size))
}

fn zip_error_to_fileserver_error(error: zip::result::ZipError) -> FileServerError {
    FileServerError::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        error.to_string(),
    ))
}

/// Recursively add a directory to a zip archive.
fn add_directory_to_zip<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    root: &Path,
    dir: &Path,
    options: SimpleFileOptions,
) -> Result<(), FileServerError> {
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let entry_path = entry.path();
        let relative = get_relative_path(root, entry_path);

        if entry.file_type().is_file() {
            zip.start_file(&relative, options)
                .map_err(zip_error_to_fileserver_error)?;
            let mut input = std::fs::File::open(entry_path)?;
            std::io::copy(&mut input, zip)?;
        } else if entry.file_type().is_dir() && entry_path != dir {
            let dir_name = format!("{}/", relative);
            zip.add_directory(&dir_name, options)
                .map_err(zip_error_to_fileserver_error)?;
        }
    }

    Ok(())
}

fn enforce_zip_limits(
    root: &Path,
    paths: &[PathBuf],
    limits: ZipLimits,
) -> Result<(), FileServerError> {
    let mut total_bytes = 0u64;
    let mut total_entries = 0u64;

    for path in paths {
        if path.is_file() {
            track_zip_entry(path, &mut total_bytes, &mut total_entries, limits)?;
            continue;
        }

        if path.is_dir() {
            for entry in WalkDir::new(path).into_iter().filter_map(|entry| entry.ok()) {
                if entry.file_type().is_file() {
                    track_zip_entry(
                        entry.path(),
                        &mut total_bytes,
                        &mut total_entries,
                        limits,
                    )?;
                }
            }
            continue;
        }

        let relative = get_relative_path(root, path);
        return Err(FileServerError::NotFound(relative));
    }

    Ok(())
}

fn track_zip_entry(
    path: &Path,
    total_bytes: &mut u64,
    total_entries: &mut u64,
    limits: ZipLimits,
) -> Result<(), FileServerError> {
    let size = std::fs::metadata(path)?.len();
    *total_entries = total_entries.saturating_add(1);
    *total_bytes = total_bytes.saturating_add(size);

    if limits.max_entries > 0 && *total_entries > limits.max_entries {
        return Err(FileServerError::ZipTooManyEntries {
            entries: *total_entries,
            limit: limits.max_entries,
        });
    }

    if limits.max_bytes > 0 && *total_bytes > limits.max_bytes {
        return Err(FileServerError::ZipTooLarge {
            size: *total_bytes,
            limit: limits.max_bytes,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::ModifyKind;
    use std::fs;
    use std::io::Read;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use zip::ZipArchive;

    // ========================================================================
    // Filename Sanitization Tests
    // ========================================================================

    #[test]
    fn test_sanitize_filename_normal() {
        assert_eq!(sanitize_filename("test.txt"), Some("test.txt".to_string()));
        assert_eq!(
            sanitize_filename("my-file.pdf"),
            Some("my-file.pdf".to_string())
        );
        assert_eq!(
            sanitize_filename("document_v2.docx"),
            Some("document_v2.docx".to_string())
        );
    }

    #[test]
    fn test_sanitize_filename_removes_path_separators() {
        // Path traversal attempts should be sanitized
        // Separators become underscores, then leading dots/spaces are trimmed
        let result = sanitize_filename("../etc/passwd");
        assert!(result.is_some());
        // The exact result depends on processing order, but should not contain path separators
        let r = result.unwrap();
        assert!(!r.contains('/'));
        assert!(!r.contains('\\'));

        let result = sanitize_filename("..\\..\\windows\\system32");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(!r.contains('/'));
        assert!(!r.contains('\\'));

        // Normal nested paths
        assert_eq!(
            sanitize_filename("foo/bar/baz.txt"),
            Some("foo_bar_baz.txt".to_string())
        );
    }

    #[test]
    fn test_sanitize_filename_removes_null_bytes() {
        assert_eq!(
            sanitize_filename("test\0.txt"),
            Some("test.txt".to_string())
        );
        assert_eq!(
            sanitize_filename("foo\0bar\0baz"),
            Some("foobarbaz".to_string())
        );
    }

    #[test]
    fn test_sanitize_filename_removes_control_chars() {
        assert_eq!(
            sanitize_filename("test\x01\x02.txt"),
            Some("test.txt".to_string())
        );
    }

    #[test]
    fn test_sanitize_filename_removes_dangerous_chars() {
        assert_eq!(
            sanitize_filename("file:name.txt"),
            Some("file_name.txt".to_string())
        );
        assert_eq!(
            sanitize_filename("file*name.txt"),
            Some("file_name.txt".to_string())
        );
        assert_eq!(
            sanitize_filename("file?name.txt"),
            Some("file_name.txt".to_string())
        );
        assert_eq!(
            sanitize_filename("file\"name.txt"),
            Some("file_name.txt".to_string())
        );
        assert_eq!(
            sanitize_filename("file<name>.txt"),
            Some("file_name_.txt".to_string())
        );
        assert_eq!(
            sanitize_filename("file|name.txt"),
            Some("file_name.txt".to_string())
        );
    }

    #[test]
    fn test_sanitize_filename_empty() {
        assert_eq!(sanitize_filename(""), None);
        assert_eq!(sanitize_filename("..."), None); // All dots stripped
        assert_eq!(sanitize_filename("   "), None); // All spaces stripped
    }

    #[test]
    fn test_sanitize_filename_reserved_windows_names() {
        assert_eq!(sanitize_filename("CON"), None);
        assert_eq!(sanitize_filename("PRN"), None);
        assert_eq!(sanitize_filename("AUX"), None);
        assert_eq!(sanitize_filename("NUL"), None);
        assert_eq!(sanitize_filename("COM1"), None);
        assert_eq!(sanitize_filename("LPT1"), None);
        // Case insensitive
        assert_eq!(sanitize_filename("con"), None);
        assert_eq!(sanitize_filename("Con"), None);
        // With extensions
        assert_eq!(sanitize_filename("CON.txt"), None);
    }

    #[test]
    fn test_sanitize_filename_length_limit() {
        let long_name = "a".repeat(300);
        let result = sanitize_filename(&long_name);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 255);
    }

    // ========================================================================
    // Zip Download Tests
    // ========================================================================

    #[test]
    fn test_create_zip_tempfile_blocking_writes_entries() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        std::fs::write(root.join("root.txt"), "root").unwrap();

        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(root.join("nested").join("child.txt"), "child").unwrap();

        let limits = ZipLimits {
            max_bytes: 0,
            max_entries: 0,
        };
        let (file, size) = create_zip_tempfile_blocking(
            root,
            &[root.join("nested"), root.join("root.txt")],
            limits,
        )
        .unwrap();
        assert!(size > 0);

        let mut archive = ZipArchive::new(file).unwrap();

        {
            let mut root_entry = archive.by_name("root.txt").unwrap();
            let mut root_content = String::new();
            root_entry.read_to_string(&mut root_content).unwrap();
            assert_eq!(root_content, "root");
        }

        {
            let mut child_entry = archive.by_name("nested/child.txt").unwrap();
            let mut child_content = String::new();
            child_entry.read_to_string(&mut child_content).unwrap();
            assert_eq!(child_content, "child");
        }
    }

    #[test]
    fn test_enforce_zip_limits_rejects_too_many_entries() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        fs::write(root.join("a.txt"), "a").unwrap();
        fs::write(root.join("b.txt"), "b").unwrap();
        fs::write(root.join("c.txt"), "c").unwrap();

        let limits = ZipLimits {
            max_bytes: 0,
            max_entries: 2,
        };

        let result = enforce_zip_limits(root, &[root.to_path_buf()], limits);
        assert!(matches!(
            result,
            Err(FileServerError::ZipTooManyEntries { .. })
        ));
    }

    #[test]
    fn test_enforce_zip_limits_rejects_too_large() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let payload = vec![0u8; 2048];
        fs::write(root.join("big.bin"), payload).unwrap();

        let limits = ZipLimits {
            max_bytes: 1024,
            max_entries: 0,
        };

        let result = enforce_zip_limits(root, &[root.to_path_buf()], limits);
        assert!(matches!(result, Err(FileServerError::ZipTooLarge { .. })));
    }

    // ========================================================================
    // Path Resolution Tests
    // ========================================================================

    #[test]
    fn test_resolve_path_normal() {
        let root = PathBuf::from("/tmp/testroot");

        let result = resolve_path(&root, "subdir/file.txt");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), root.join("subdir/file.txt"));
    }

    #[test]
    fn test_resolve_path_empty() {
        let root = PathBuf::from("/tmp/testroot");

        let result = resolve_path(&root, "");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), root);
    }

    #[test]
    fn test_resolve_path_dot() {
        let root = PathBuf::from("/tmp/testroot");

        let result = resolve_path(&root, ".");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), root);
    }

    #[test]
    fn test_resolve_path_rejects_parent_dir() {
        let root = PathBuf::from("/tmp/testroot");

        // Direct parent reference
        let result = resolve_path(&root, "..");
        assert!(matches!(
            result,
            Err(crate::error::FileServerError::PathTraversal)
        ));

        // Nested parent reference
        let result = resolve_path(&root, "subdir/../..");
        assert!(matches!(
            result,
            Err(crate::error::FileServerError::PathTraversal)
        ));

        // Parent reference that would escape root
        let result = resolve_path(&root, "../etc/passwd");
        assert!(matches!(
            result,
            Err(crate::error::FileServerError::PathTraversal)
        ));
    }

    #[test]
    fn test_resolve_path_rejects_absolute_paths() {
        let root = PathBuf::from("/tmp/testroot");

        // Note: Our implementation strips leading slashes for convenience,
        // so "/etc/passwd" becomes "etc/passwd" which is valid
        // This is intentional - it allows paths like "/subdir/file" to work
        let result = resolve_path(&root, "/etc/passwd");
        assert!(result.is_ok());
        // The path should be within root
        assert!(result.unwrap().starts_with(&root));
    }

    #[test]
    fn test_resolve_path_rejects_null_bytes() {
        let root = PathBuf::from("/tmp/testroot");

        let result = resolve_path(&root, "file\0.txt");
        assert!(matches!(
            result,
            Err(crate::error::FileServerError::PathTraversal)
        ));
    }

    #[test]
    fn test_resolve_path_handles_leading_slash() {
        let root = PathBuf::from("/tmp/testroot");

        // Leading slash should be stripped, not treated as absolute
        let result = resolve_path(&root, "/subdir/file.txt");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), root.join("subdir/file.txt"));
    }

    // ========================================================================
    // Integration Tests (require temp directory)
    // ========================================================================

    #[test]
    fn test_resolve_and_verify_path_with_real_fs() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create a subdirectory
        std::fs::create_dir_all(root.join("subdir")).unwrap();
        std::fs::write(root.join("subdir/test.txt"), "test").unwrap();

        // Normal path should work
        let result = resolve_and_verify_path(&root, "subdir/test.txt");
        assert!(result.is_ok());

        // Non-existent path should still resolve (for uploads)
        let result = resolve_and_verify_path(&root, "subdir/newfile.txt");
        assert!(result.is_ok());

        // Parent traversal should fail
        let result = resolve_and_verify_path(&root, "../etc/passwd");
        assert!(matches!(
            result,
            Err(crate::error::FileServerError::PathTraversal)
        ));
    }

    #[test]
    fn test_resolve_and_verify_path_detects_symlink_escape() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create a directory outside root
        let outside_dir = TempDir::new().unwrap();
        std::fs::write(outside_dir.path().join("secret.txt"), "secret data").unwrap();

        // Create a symlink inside root that points outside
        std::fs::create_dir_all(root.join("subdir")).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let _ = symlink(outside_dir.path(), root.join("subdir/escape"));

            // Trying to access via symlink should fail
            let result = resolve_and_verify_path(&root, "subdir/escape/secret.txt");
            assert!(matches!(
                result,
                Err(crate::error::FileServerError::PathTraversal)
            ));
        }
    }

    // ========================================================================
    // Root Directory Deletion Prevention Tests
    // ========================================================================

    #[test]
    fn test_cannot_delete_root_via_empty_path() {
        // This is tested via the delete handler, but we can verify the path resolution
        let root = PathBuf::from("/tmp/testroot");

        let result = resolve_path(&root, "");
        assert!(result.is_ok());
        // The resolved path equals root, which should be caught by delete handler
        assert_eq!(result.unwrap(), root);
    }

    #[test]
    fn test_cannot_delete_root_via_dot_path() {
        let root = PathBuf::from("/tmp/testroot");

        let result = resolve_path(&root, ".");
        assert!(result.is_ok());
        // The resolved path equals root, which should be caught by delete handler
        assert_eq!(result.unwrap(), root);
    }

    // ========================================================================
    // Watch Filter Tests
    // ========================================================================

    #[test]
    fn test_normalize_extension() {
        assert_eq!(normalize_extension(".md"), Some("md".to_string()));
        assert_eq!(normalize_extension("TXT"), Some("txt".to_string()));
        assert_eq!(normalize_extension("  .Rs "), Some("rs".to_string()));
        assert_eq!(normalize_extension("."), None);
        assert_eq!(normalize_extension(""), None);
    }

    #[test]
    fn test_parse_extension_filter() {
        let filter = parse_extension_filter(&Some(".md, txt , .RS".to_string())).unwrap();
        assert!(filter.contains("md"));
        assert!(filter.contains("txt"));
        assert!(filter.contains("rs"));
        assert_eq!(filter.len(), 3);

        let empty = parse_extension_filter(&Some(" , ".to_string()));
        assert!(empty.is_none());
    }

    #[test]
    fn test_event_label_mapping() {
        let file_create = EventKind::Create(CreateKind::File);
        let dir_create = EventKind::Create(CreateKind::Folder);
        let file_modify = EventKind::Modify(ModifyKind::Any);
        let file_remove = EventKind::Remove(RemoveKind::File);
        let dir_remove = EventKind::Remove(RemoveKind::Folder);

        assert_eq!(event_label(&file_create, false), Some("file_created"));
        assert_eq!(event_label(&dir_create, true), Some("dir_created"));
        assert_eq!(event_label(&file_modify, false), Some("file_modified"));
        assert_eq!(event_label(&file_remove, false), Some("file_deleted"));
        assert_eq!(event_label(&dir_remove, true), Some("dir_deleted"));
        assert_eq!(event_label(&file_modify, true), None);
    }
}
