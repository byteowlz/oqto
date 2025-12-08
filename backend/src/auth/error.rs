//! Authentication errors.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;

/// Authentication errors.
#[derive(Debug, Error)]
pub enum AuthError {
    /// Missing authorization header.
    #[error("missing authorization header")]
    MissingAuthHeader,

    /// Invalid authorization header format.
    #[error("invalid authorization header format")]
    InvalidAuthHeader,

    /// Invalid token.
    #[error("invalid token: {0}")]
    InvalidToken(String),

    /// Token expired.
    #[error("token expired")]
    TokenExpired,

    /// Insufficient permissions.
    #[error("insufficient permissions: {0}")]
    InsufficientPermissions(String),

    /// User not found.
    #[error("user not found")]
    UserNotFound,

    /// Invalid credentials.
    #[error("invalid credentials")]
    InvalidCredentials,

    /// Internal error.
    #[error("internal auth error: {0}")]
    Internal(String),
}

/// Error response body.
#[derive(Debug, Serialize)]
pub struct AuthErrorResponse {
    pub error: String,
    pub error_code: String,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_code) = match &self {
            AuthError::MissingAuthHeader => (StatusCode::UNAUTHORIZED, "missing_auth_header"),
            AuthError::InvalidAuthHeader => (StatusCode::UNAUTHORIZED, "invalid_auth_header"),
            AuthError::InvalidToken(_) => (StatusCode::UNAUTHORIZED, "invalid_token"),
            AuthError::TokenExpired => (StatusCode::UNAUTHORIZED, "token_expired"),
            AuthError::InsufficientPermissions(_) => {
                (StatusCode::FORBIDDEN, "insufficient_permissions")
            }
            AuthError::UserNotFound => (StatusCode::NOT_FOUND, "user_not_found"),
            AuthError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
            AuthError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };

        let body = Json(AuthErrorResponse {
            error: self.to_string(),
            error_code: error_code.to_string(),
        });

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_error_display() {
        let err = AuthError::MissingAuthHeader;
        assert_eq!(err.to_string(), "missing authorization header");

        let err = AuthError::InvalidToken("bad".to_string());
        assert_eq!(err.to_string(), "invalid token: bad");
    }
}
