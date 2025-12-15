use std::path::{Path, PathBuf};

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
use tracing::{debug, error, info};
use walkdir::WalkDir;

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

// ============================================================================
// Helper functions
// ============================================================================

/// Resolve and validate a path, ensuring it's within the root directory
fn resolve_path(root: &Path, relative: &str) -> Result<PathBuf, FileServerError> {
    // Normalize the relative path
    let relative = relative.trim_start_matches('/');
    let relative = if relative.is_empty() || relative == "." {
        PathBuf::new()
    } else {
        PathBuf::from(relative)
    };

    // Join with root and canonicalize
    let full_path = root.join(&relative);

    // For paths that don't exist yet (uploads), we check the parent
    let check_path = if full_path.exists() {
        full_path.canonicalize().map_err(FileServerError::Io)?
    } else {
        // For new files, verify the parent directory is valid
        let parent = full_path.parent().ok_or_else(|| {
            FileServerError::InvalidPath("Invalid parent directory".to_string())
        })?;

        if parent.exists() {
            let canonical_parent = parent.canonicalize().map_err(FileServerError::Io)?;
            if !canonical_parent.starts_with(root) {
                return Err(FileServerError::PathTraversal);
            }
            full_path
        } else {
            full_path
        }
    };

    // Security check: ensure resolved path is within root
    let canonical_root = root.canonicalize().map_err(FileServerError::Io)?;
    if check_path.exists() && !check_path.starts_with(&canonical_root) {
        return Err(FileServerError::PathTraversal);
    }

    // Also check for path traversal attempts in the relative path itself
    for component in relative.components() {
        if let std::path::Component::ParentDir = component {
            // Allow .. only if the resolved path is still within root
            if check_path.exists() {
                let resolved = check_path.canonicalize().map_err(FileServerError::Io)?;
                if !resolved.starts_with(&canonical_root) {
                    return Err(FileServerError::PathTraversal);
                }
            }
        }
    }

    Ok(check_path)
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
    let path = resolve_path(&state.root_dir, &query.path)?;

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
            let tree = build_tree(&state, &path, max_depth, query.show_hidden)?;
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
pub async fn get_file(
    State(state): State<AppState>,
    Query(query): Query<FileQuery>,
) -> Result<Response, FileServerError> {
    let path = resolve_path(&state.root_dir, &query.path)?;

    if !path.exists() {
        return Err(FileServerError::NotFound(query.path));
    }

    if path.is_dir() {
        return Err(FileServerError::NotAFile);
    }

    debug!("Reading file: {}", path.display());

    let content = fs::read(&path).await.map_err(FileServerError::Io)?;
    let mime = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .to_string();

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"{}\"", file_name),
            ),
        ],
        Body::from(content),
    )
        .into_response())
}

/// POST /file - Upload file
pub async fn upload_file(
    State(state): State<AppState>,
    Query(query): Query<UploadQuery>,
    mut multipart: Multipart,
) -> Result<Json<SuccessResponse>, FileServerError> {
    let dest_path = resolve_path(&state.root_dir, &query.path)?;

    // Create parent directories if requested
    if query.mkdir {
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                error!("Failed to create directory: {}", e);
                FileServerError::CreateDirFailed(parent.display().to_string())
            })?;
        }
    }

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        error!("Multipart error: {}", e);
        FileServerError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    })? {
        let file_name = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "upload".to_string());

        // Determine final path
        let final_path = if dest_path.is_dir() || query.path.ends_with('/') {
            // If destination is a directory, use the uploaded filename
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

        // Validate the final path is within root
        let canonical_root = state.root_dir.canonicalize().map_err(FileServerError::Io)?;
        if let Some(parent) = final_path.parent() {
            if parent.exists() {
                let canonical_parent = parent.canonicalize().map_err(FileServerError::Io)?;
                if !canonical_parent.starts_with(&canonical_root) {
                    return Err(FileServerError::PathTraversal);
                }
            }
        }

        let data = field.bytes().await.map_err(|e| {
            error!("Failed to read upload data: {}", e);
            FileServerError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;

        // Check file size
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
    let path = resolve_path(&state.root_dir, &query.path)?;

    if !path.exists() {
        return Err(FileServerError::NotFound(query.path));
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
