use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FileServerError {
    #[error("File not found: {0}")]
    NotFound(String),

    #[error("Path is outside root directory")]
    PathTraversal,

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("File too large: {size} bytes exceeds limit of {limit} bytes")]
    FileTooLarge { size: u64, limit: u64 },

    #[error("Directory operation not allowed on file")]
    NotADirectory,

    #[error("File operation not allowed on directory")]
    NotAFile,

    #[error("Failed to create directory: {0}")]
    CreateDirFailed(String),
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    code: &'static str,
}

impl IntoResponse for FileServerError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            FileServerError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            FileServerError::PathTraversal => (StatusCode::FORBIDDEN, "PATH_TRAVERSAL"),
            FileServerError::InvalidPath(_) => (StatusCode::BAD_REQUEST, "INVALID_PATH"),
            FileServerError::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "IO_ERROR"),
            FileServerError::FileTooLarge { .. } => {
                (StatusCode::PAYLOAD_TOO_LARGE, "FILE_TOO_LARGE")
            }
            FileServerError::NotADirectory => (StatusCode::BAD_REQUEST, "NOT_A_DIRECTORY"),
            FileServerError::NotAFile => (StatusCode::BAD_REQUEST, "NOT_A_FILE"),
            FileServerError::CreateDirFailed(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "CREATE_DIR_FAILED")
            }
        };

        let body = ErrorResponse {
            error: self.to_string(),
            code,
        };

        (status, Json(body)).into_response()
    }
}
