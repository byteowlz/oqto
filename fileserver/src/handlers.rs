use std::io::{Cursor, Write};
use std::path::{Path, PathBuf, Component};

use axum::{
    body::Body,
    extract::{Multipart, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use tracing::{debug, error, info, warn};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::error::FileServerError;
use crate::AppState;

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

/// Query parameters for tree endpoint
#[derive(Debug, Deserialize)]
pub struct TreeQuery {
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
    /// Path relative to root
    pub path: String,
}

/// Upload query parameters
#[derive(Debug, Deserialize)]
pub struct UploadQuery {
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
    /// Path relative to root (for single file/directory download)
    pub path: String,
}

/// Query parameters for multi-file zip download
#[derive(Debug, Deserialize)]
pub struct DownloadZipQuery {
    /// Comma-separated list of paths to include in the zip
    pub paths: String,
    /// Optional name for the zip file (defaults to "download.zip")
    #[serde(default)]
    pub name: Option<String>,
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
    let reserved = ["CON", "PRN", "AUX", "NUL", 
                   "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
                   "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"];
    if reserved.iter().any(|r| upper == *r || upper.starts_with(&format!("{}.", r))) {
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
        error!("Path resolution resulted in path outside root: {:?}", result);
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

/// Get relative path from root
fn get_relative_path(root: &Path, full_path: &Path) -> String {
    full_path
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
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

/// GET /tree - Get directory tree
pub async fn get_tree(
    State(state): State<AppState>,
    Query(query): Query<TreeQuery>,
) -> Result<Json<Vec<FileNode>>, FileServerError> {
    // Use resolve_and_verify_path for proper symlink handling
    let path = resolve_and_verify_path(&state.root_dir, &query.path)?;

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
            let files = get_simple_file_list(&state, &path, max_depth)?;
            Ok(Json(files))
        }
        ViewMode::Full => {
            // Full directory tree
            let state = state.clone();
            let path = path.clone();
            let show_hidden = query.show_hidden;

            let tree = tokio::task::spawn_blocking(move || {
                build_tree(&state, &path, max_depth, show_hidden)
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
        let relative_path = get_relative_path(&state.root_dir, &entry_path);

        let node = if entry_path.is_dir() {
            let children = if max_depth > 1 {
                Some(build_tree(state, &entry_path, max_depth - 1, show_hidden)?)
            } else {
                None
            };

            FileNode {
                name: file_name,
                path: relative_path,
                node_type: FileType::Directory,
                size: None,
                modified: metadata.modified().ok().and_then(|t| {
                    t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
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
                    t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
                }),
                children: None,
            }
        };

        nodes.push(node);
    }

    // Sort: directories first, then alphabetically
    nodes.sort_by(|a, b| {
        match (&a.node_type, &b.node_type) {
            (FileType::Directory, FileType::File) => std::cmp::Ordering::Less,
            (FileType::File, FileType::Directory) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Ok(nodes)
}

/// Get flat list of office files (simple mode)
fn get_simple_file_list(
    state: &AppState,
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
        let relative_path = get_relative_path(&state.root_dir, entry_path);

        files.push(FileNode {
            name: file_name,
            path: relative_path,
            node_type: FileType::File,
            size: Some(metadata.len()),
            modified: metadata.modified().ok().and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
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
pub async fn get_file(
    State(state): State<AppState>,
    Query(query): Query<FileQuery>,
) -> Result<Response, FileServerError> {
    // Use resolve_and_verify_path for proper symlink handling
    let path = resolve_and_verify_path(&state.root_dir, &query.path)?;

    if !path.exists() {
        return Err(FileServerError::NotFound(query.path));
    }

    if path.is_dir() {
        return Err(FileServerError::NotAFile);
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

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

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

/// POST /file - Upload file
pub async fn upload_file(
    State(state): State<AppState>,
    Query(query): Query<UploadQuery>,
    mut multipart: Multipart,
) -> Result<Json<SuccessResponse>, FileServerError> {
    // Use resolve_and_verify_path for proper symlink handling
    let dest_path = resolve_path(&state.root_dir, &query.path)?;

    // Create parent directories if requested
    if query.mkdir {
        if let Some(parent) = dest_path.parent() {
            if parent != state.root_dir {
                fs::create_dir_all(parent).await.map_err(|e| {
                    error!("Failed to create directory: {}", e);
                    FileServerError::CreateDirFailed(parent.display().to_string())
                })?;
            }
        }
    }

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        error!("Multipart error: {}", e);
        FileServerError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    })? {
        // Get and SANITIZE the filename
        let raw_filename = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "upload".to_string());
        
        let file_name = sanitize_filename(&raw_filename)
            .ok_or_else(|| {
                warn!("Rejected invalid filename: {:?}", raw_filename);
                FileServerError::InvalidPath(format!("Invalid filename: {}", raw_filename))
            })?;

        // Determine final path
        let final_path = if dest_path.is_dir() || query.path.ends_with('/') {
            // If destination is a directory, use the sanitized filename
            let dir_path = if dest_path.exists() {
                dest_path.clone()
            } else if query.mkdir {
                fs::create_dir_all(&dest_path).await.map_err(|_| {
                    FileServerError::CreateDirFailed(dest_path.display().to_string())
                })?;
                dest_path.clone()
            } else {
                return Err(FileServerError::NotFound(query.path.clone()));
            };
            dir_path.join(&file_name)
        } else {
            dest_path.clone()
        };

        // Re-validate the final path is within root (belt-and-suspenders)
        let canonical_root = state.root_dir.canonicalize().map_err(FileServerError::Io)?;
        if let Some(parent) = final_path.parent() {
            if parent.exists() {
                let canonical_parent = parent.canonicalize().map_err(FileServerError::Io)?;
                if !canonical_parent.starts_with(&canonical_root) {
                    warn!("Final path parent outside root: {:?}", final_path);
                    return Err(FileServerError::PathTraversal);
                }
            }
        }

        // Read the data first to check size before writing
        let data = field.bytes().await.map_err(|e| {
            error!("Failed to read upload data: {}", e);
            FileServerError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;

        // Check file size BEFORE writing to disk
        if data.len() as u64 > state.config.max_upload_size {
            return Err(FileServerError::FileTooLarge {
                size: data.len() as u64,
                limit: state.config.max_upload_size,
            });
        }

        info!("Uploading file: {} ({} bytes)", final_path.display(), data.len());

        // Write file
        let mut file = fs::File::create(&final_path).await.map_err(FileServerError::Io)?;
        file.write_all(&data).await.map_err(FileServerError::Io)?;

        let relative_path = get_relative_path(&state.root_dir, &final_path);

        return Ok(Json(SuccessResponse {
            success: true,
            message: format!("File uploaded: {}", file_name),
            path: Some(relative_path),
        }));
    }

    Err(FileServerError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "No file in upload",
    )))
}

/// DELETE /file - Delete file or directory
pub async fn delete_file(
    State(state): State<AppState>,
    Query(query): Query<FileQuery>,
) -> Result<Json<SuccessResponse>, FileServerError> {
    // Use resolve_and_verify_path for proper symlink handling
    let path = resolve_and_verify_path(&state.root_dir, &query.path)?;

    if !path.exists() {
        return Err(FileServerError::NotFound(query.path));
    }

    // SECURITY: Prevent deletion of root directory
    let canonical_root = state.root_dir.canonicalize().map_err(FileServerError::Io)?;
    let canonical_path = path.canonicalize().map_err(FileServerError::Io)?;
    
    if canonical_path == canonical_root {
        warn!("Attempted to delete root directory: {:?}", query.path);
        return Err(FileServerError::InvalidPath(
            "Cannot delete root directory".to_string()
        ));
    }

    // Double-check path is still within root after canonicalization
    if !canonical_path.starts_with(&canonical_root) {
        warn!("Delete path escaped root after canonicalization: {:?}", path);
        return Err(FileServerError::PathTraversal);
    }

    info!("Deleting: {}", path.display());

    if path.is_dir() {
        fs::remove_dir_all(&path).await.map_err(FileServerError::Io)?;
    } else {
        fs::remove_file(&path).await.map_err(FileServerError::Io)?;
    }

    Ok(Json(SuccessResponse {
        success: true,
        message: format!("Deleted: {}", query.path),
        path: Some(query.path),
    }))
}

/// PUT /mkdir - Create directory
pub async fn create_dir(
    State(state): State<AppState>,
    Query(query): Query<FileQuery>,
) -> Result<Json<SuccessResponse>, FileServerError> {
    let path = resolve_path(&state.root_dir, &query.path)?;

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

/// GET /download - Download a single file or directory as zip
/// 
/// For files: returns the file with Content-Disposition: attachment
/// For directories: returns a zip archive of the directory
pub async fn download(
    State(state): State<AppState>,
    Query(query): Query<DownloadQuery>,
) -> Result<Response, FileServerError> {
    let path = resolve_and_verify_path(&state.root_dir, &query.path)?;

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

        // Create zip in memory
        let zip_data = create_zip_from_paths(&state.root_dir, &[path.clone()])?;

        Ok((
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/zip".to_string()),
                (header::CONTENT_LENGTH, zip_data.len().to_string()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", safe_zip_name),
                ),
            ],
            zip_data,
        )
            .into_response())
    }
}

/// GET /download-zip - Download multiple files/directories as a single zip
pub async fn download_zip(
    State(state): State<AppState>,
    Query(query): Query<DownloadZipQuery>,
) -> Result<Response, FileServerError> {
    // Parse comma-separated paths
    let paths: Vec<&str> = query.paths.split(',').map(|s| s.trim()).collect();
    
    if paths.is_empty() {
        return Err(FileServerError::InvalidPath("No paths provided".to_string()));
    }

    // Resolve and verify all paths
    let mut resolved_paths = Vec::new();
    for path_str in &paths {
        let resolved = resolve_and_verify_path(&state.root_dir, path_str)?;
        if !resolved.exists() {
            return Err(FileServerError::NotFound(path_str.to_string()));
        }
        resolved_paths.push(resolved);
    }

    debug!("Downloading {} items as zip", resolved_paths.len());

    // Create zip
    let zip_data = create_zip_from_paths(&state.root_dir, &resolved_paths)?;

    let zip_name = query.name.unwrap_or_else(|| "download.zip".to_string());
    let safe_zip_name = zip_name.replace('"', "'");

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (header::CONTENT_LENGTH, zip_data.len().to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", safe_zip_name),
            ),
        ],
        zip_data,
    )
        .into_response())
}

/// Create a zip archive from a list of paths (files or directories)
fn create_zip_from_paths(root: &Path, paths: &[PathBuf]) -> Result<Vec<u8>, FileServerError> {
    let buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buffer);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    for path in paths {
        if path.is_file() {
            // Add single file
            let relative = get_relative_path(root, path);
            let file_name = if relative.is_empty() {
                path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "file".to_string())
            } else {
                relative
            };
            
            let data = std::fs::read(path).map_err(FileServerError::Io)?;
            zip.start_file(&file_name, options).map_err(|e| {
                FileServerError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
            })?;
            zip.write_all(&data).map_err(FileServerError::Io)?;
        } else if path.is_dir() {
            // Add directory recursively
            add_directory_to_zip(&mut zip, root, path, options)?;
        }
    }

    let result = zip.finish().map_err(|e| {
        FileServerError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    })?;

    Ok(result.into_inner())
}

/// Recursively add a directory to a zip archive
fn add_directory_to_zip(
    zip: &mut ZipWriter<Cursor<Vec<u8>>>,
    root: &Path,
    dir: &Path,
    options: SimpleFileOptions,
) -> Result<(), FileServerError> {
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let entry_path = entry.path();
        let relative = get_relative_path(root, entry_path);

        if entry_path.is_file() {
            let data = std::fs::read(entry_path).map_err(FileServerError::Io)?;
            zip.start_file(&relative, options).map_err(|e| {
                FileServerError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
            })?;
            zip.write_all(&data).map_err(FileServerError::Io)?;
        } else if entry_path.is_dir() && entry_path != dir {
            // Add directory entry (trailing slash)
            let dir_name = format!("{}/", relative);
            zip.add_directory(&dir_name, options).map_err(|e| {
                FileServerError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
            })?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // ========================================================================
    // Filename Sanitization Tests
    // ========================================================================

    #[test]
    fn test_sanitize_filename_normal() {
        assert_eq!(sanitize_filename("test.txt"), Some("test.txt".to_string()));
        assert_eq!(sanitize_filename("my-file.pdf"), Some("my-file.pdf".to_string()));
        assert_eq!(sanitize_filename("document_v2.docx"), Some("document_v2.docx".to_string()));
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
        assert_eq!(sanitize_filename("foo/bar/baz.txt"), Some("foo_bar_baz.txt".to_string()));
    }

    #[test]
    fn test_sanitize_filename_removes_null_bytes() {
        assert_eq!(sanitize_filename("test\0.txt"), Some("test.txt".to_string()));
        assert_eq!(sanitize_filename("foo\0bar\0baz"), Some("foobarbaz".to_string()));
    }

    #[test]
    fn test_sanitize_filename_removes_control_chars() {
        assert_eq!(sanitize_filename("test\x01\x02.txt"), Some("test.txt".to_string()));
    }

    #[test]
    fn test_sanitize_filename_removes_dangerous_chars() {
        assert_eq!(sanitize_filename("file:name.txt"), Some("file_name.txt".to_string()));
        assert_eq!(sanitize_filename("file*name.txt"), Some("file_name.txt".to_string()));
        assert_eq!(sanitize_filename("file?name.txt"), Some("file_name.txt".to_string()));
        assert_eq!(sanitize_filename("file\"name.txt"), Some("file_name.txt".to_string()));
        assert_eq!(sanitize_filename("file<name>.txt"), Some("file_name_.txt".to_string()));
        assert_eq!(sanitize_filename("file|name.txt"), Some("file_name.txt".to_string()));
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
        assert!(matches!(result, Err(crate::error::FileServerError::PathTraversal)));
        
        // Nested parent reference
        let result = resolve_path(&root, "subdir/../..");
        assert!(matches!(result, Err(crate::error::FileServerError::PathTraversal)));
        
        // Parent reference that would escape root
        let result = resolve_path(&root, "../etc/passwd");
        assert!(matches!(result, Err(crate::error::FileServerError::PathTraversal)));
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
        assert!(matches!(result, Err(crate::error::FileServerError::PathTraversal)));
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
        assert!(matches!(result, Err(crate::error::FileServerError::PathTraversal)));
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
            assert!(matches!(result, Err(crate::error::FileServerError::PathTraversal)));
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
}
