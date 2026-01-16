//! Main Chat file server handlers.
//!
//! Provides file access for the Main Chat workspace using octo-files handlers.

use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use tracing::warn;

use super::state::AppState;
use crate::auth::CurrentUser;

/// Get the octo-files AppState for a user's Main Chat directory.
fn get_files_state(state: &AppState, user_id: &str) -> Result<octo_files::AppState, StatusCode> {
    let main_chat = state
        .main_chat
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let root_dir = main_chat.get_main_chat_dir(user_id);

    if !root_dir.exists() {
        warn!(
            "Main Chat directory does not exist for user {}: {:?}",
            user_id, root_dir
        );
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(octo_files::AppState::new(root_dir))
}

/// GET /main/files/tree - Get directory tree for Main Chat workspace.
pub async fn get_tree(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<octo_files::handlers::TreeQuery>,
) -> Result<Response, StatusCode> {
    let files_state = get_files_state(&state, user.id())?;
    octo_files::handlers::get_tree(State(files_state), Query(query))
        .await
        .map(|json| json.into_response())
        .map_err(|e| {
            warn!("Main Chat file tree error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// GET /main/files/file - Get file content from Main Chat workspace.
pub async fn get_file(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<octo_files::handlers::FileQuery>,
) -> Result<Response, StatusCode> {
    let files_state = get_files_state(&state, user.id())?;
    octo_files::handlers::get_file(State(files_state), Query(query))
        .await
        .map_err(|e| {
            warn!("Main Chat get file error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// POST /main/files/file - Upload file to Main Chat workspace.
pub async fn upload_file(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<octo_files::handlers::UploadQuery>,
    multipart: axum::extract::Multipart,
) -> Result<Response, StatusCode> {
    let files_state = get_files_state(&state, user.id())?;
    octo_files::handlers::upload_file(State(files_state), Query(query), multipart)
        .await
        .map(|json| json.into_response())
        .map_err(|e| {
            warn!("Main Chat upload file error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// PUT /main/files/file - Write file content to Main Chat workspace.
pub async fn write_file(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<octo_files::handlers::UploadQuery>,
    body: axum::body::Bytes,
) -> Result<Response, StatusCode> {
    let files_state = get_files_state(&state, user.id())?;
    octo_files::handlers::write_file(State(files_state), Query(query), body)
        .await
        .map(|json| json.into_response())
        .map_err(|e| {
            warn!("Main Chat write file error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// DELETE /main/files/file - Delete file from Main Chat workspace.
pub async fn delete_file(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<octo_files::handlers::FileQuery>,
) -> Result<Response, StatusCode> {
    let files_state = get_files_state(&state, user.id())?;
    octo_files::handlers::delete_file(State(files_state), Query(query))
        .await
        .map(|json| json.into_response())
        .map_err(|e| {
            warn!("Main Chat delete file error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// PUT /main/files/mkdir - Create directory in Main Chat workspace.
pub async fn create_dir(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<octo_files::handlers::FileQuery>,
) -> Result<Response, StatusCode> {
    let files_state = get_files_state(&state, user.id())?;
    octo_files::handlers::create_dir(State(files_state), Query(query))
        .await
        .map(|json| json.into_response())
        .map_err(|e| {
            warn!("Main Chat create dir error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// POST /main/files/rename - Rename file or directory in Main Chat workspace.
pub async fn rename_file(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<octo_files::handlers::RenameQuery>,
) -> Result<Response, StatusCode> {
    let files_state = get_files_state(&state, user.id())?;
    octo_files::handlers::rename_file(State(files_state), Query(query))
        .await
        .map(|json| json.into_response())
        .map_err(|e| {
            warn!("Main Chat rename file error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// GET /main/files/download - Download file or directory from Main Chat workspace.
pub async fn download(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<octo_files::handlers::DownloadQuery>,
) -> Result<Response, StatusCode> {
    let files_state = get_files_state(&state, user.id())?;
    octo_files::handlers::download(State(files_state), Query(query))
        .await
        .map_err(|e| {
            warn!("Main Chat download error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// GET /main/files/download-zip - Download multiple files as zip from Main Chat workspace.
pub async fn download_zip(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<octo_files::handlers::DownloadZipQuery>,
) -> Result<Response, StatusCode> {
    let files_state = get_files_state(&state, user.id())?;
    octo_files::handlers::download_zip(State(files_state), Query(query))
        .await
        .map_err(|e| {
            warn!("Main Chat download zip error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// Create Main Chat file routes.
pub fn main_chat_file_routes() -> Router<AppState> {
    Router::new()
        .route("/tree", get(get_tree))
        .route(
            "/file",
            get(get_file)
                .post(upload_file)
                .put(write_file)
                .delete(delete_file),
        )
        .route("/mkdir", put(create_dir))
        .route("/rename", post(rename_file))
        .route("/download", get(download))
        .route("/download-zip", get(download_zip))
}
