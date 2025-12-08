//! Unified API error handling with structured responses.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;
use tracing::{error, warn};

/// API error type with structured responses.
#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum ApiError {
    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Gateway error: {0}")]
    BadGateway(String),
}

#[allow(dead_code)]
impl ApiError {
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::Unauthorized(msg.into())
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }

    pub fn service_unavailable(msg: impl Into<String>) -> Self {
        Self::ServiceUnavailable(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    pub fn bad_gateway(msg: impl Into<String>) -> Self {
        Self::BadGateway(msg.into())
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::BadGateway(_) => StatusCode::BAD_GATEWAY,
        }
    }

    fn error_code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "NOT_FOUND",
            Self::BadRequest(_) => "BAD_REQUEST",
            Self::Unauthorized(_) => "UNAUTHORIZED",
            Self::Forbidden(_) => "FORBIDDEN",
            Self::Conflict(_) => "CONFLICT",
            Self::ServiceUnavailable(_) => "SERVICE_UNAVAILABLE",
            Self::Internal(_) => "INTERNAL_ERROR",
            Self::BadGateway(_) => "BAD_GATEWAY",
        }
    }

    /// Categorize an anyhow error into the appropriate ApiError variant.
    /// This uses pattern matching on error messages to determine the category.
    /// 
    /// Patterns recognized:
    /// - "not found" -> NotFound
    /// - "already taken" / "already registered" / "already exists" -> Conflict  
    /// - "invalid" / validation errors -> BadRequest
    /// - "active" (for session operations) -> Conflict
    /// - "unauthorized" / "authentication" -> Unauthorized
    /// - "forbidden" / "permission" -> Forbidden
    /// - "unavailable" / "connection refused" -> ServiceUnavailable
    /// - Default -> Internal
    pub fn from_anyhow(err: anyhow::Error) -> Self {
        let msg = err.to_string();
        let msg_lower = msg.to_lowercase();

        // Check for specific patterns in priority order
        if msg_lower.contains("not found") {
            ApiError::NotFound(msg)
        } else if msg_lower.contains("already taken") 
            || msg_lower.contains("already registered")
            || msg_lower.contains("already exists")
            || (msg_lower.contains("active") && (msg_lower.contains("session") || msg_lower.contains("delete")))
        {
            ApiError::Conflict(msg)
        } else if msg_lower.contains("invalid") 
            || msg_lower.contains("must be")
            || msg_lower.contains("cannot")
            || msg_lower.contains("does not exist") // e.g., "workspace path does not exist"
        {
            ApiError::BadRequest(msg)
        } else if msg_lower.contains("unauthorized") || msg_lower.contains("authentication") {
            ApiError::Unauthorized(msg)
        } else if msg_lower.contains("forbidden") || msg_lower.contains("permission") {
            ApiError::Forbidden(msg)
        } else if msg_lower.contains("unavailable") || msg_lower.contains("connection refused") {
            ApiError::ServiceUnavailable(msg)
        } else {
            ApiError::Internal(msg)
        }
    }
}

/// Structured error response.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let code = self.error_code();
        let message = self.to_string();

        // Log errors appropriately
        match &self {
            ApiError::Internal(msg) | ApiError::BadGateway(msg) => {
                error!(error_code = code, message = %msg, "API error");
            }
            ApiError::ServiceUnavailable(msg) => {
                warn!(error_code = code, message = %msg, "Service unavailable");
            }
            _ => {
                tracing::debug!(error_code = code, message = %message, "Client error");
            }
        }

        let body = ErrorResponse {
            error: message,
            code,
            details: None,
        };

        (status, Json(body)).into_response()
    }
}

/// Convert anyhow errors to API errors using the centralized categorization logic.
impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        Self::from_anyhow(err)
    }
}

/// Convert auth errors to API errors.
impl From<crate::auth::AuthError> for ApiError {
    fn from(err: crate::auth::AuthError) -> Self {
        use crate::auth::AuthError;
        match err {
            AuthError::MissingAuthHeader | AuthError::InvalidAuthHeader => {
                ApiError::Unauthorized("Missing or invalid authorization".to_string())
            }
            AuthError::InvalidToken(msg) => {
                ApiError::Unauthorized(format!("Invalid token: {}", msg))
            }
            AuthError::TokenExpired => {
                ApiError::Unauthorized("Token has expired".to_string())
            }
            AuthError::InvalidCredentials => {
                ApiError::Unauthorized("Invalid credentials".to_string())
            }
            AuthError::UserNotFound => {
                ApiError::Unauthorized("User not found".to_string())
            }
            AuthError::InsufficientPermissions(msg) => {
                ApiError::Forbidden(msg)
            }
            AuthError::Internal(msg) => {
                ApiError::Internal(format!("Authentication error: {}", msg))
            }
        }
    }
}

/// Result type for API handlers
pub type ApiResult<T> = Result<T, ApiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_categorization_not_found() {
        let err = anyhow::anyhow!("Session not found: abc123");
        let api_err = ApiError::from_anyhow(err);
        assert!(matches!(api_err, ApiError::NotFound(_)));
    }

    #[test]
    fn test_error_categorization_conflict_taken() {
        let err = anyhow::anyhow!("Username 'admin' is already taken.");
        let api_err = ApiError::from_anyhow(err);
        assert!(matches!(api_err, ApiError::Conflict(_)));
    }

    #[test]
    fn test_error_categorization_conflict_registered() {
        let err = anyhow::anyhow!("Email 'user@example.com' is already registered.");
        let api_err = ApiError::from_anyhow(err);
        assert!(matches!(api_err, ApiError::Conflict(_)));
    }

    #[test]
    fn test_error_categorization_conflict_active_session() {
        let err = anyhow::anyhow!("cannot delete active session, stop it first");
        let api_err = ApiError::from_anyhow(err);
        assert!(matches!(api_err, ApiError::Conflict(_)));
    }

    #[test]
    fn test_error_categorization_bad_request_invalid() {
        let err = anyhow::anyhow!("Invalid username format.");
        let api_err = ApiError::from_anyhow(err);
        assert!(matches!(api_err, ApiError::BadRequest(_)));
    }

    #[test]
    fn test_error_categorization_bad_request_must_be() {
        let err = anyhow::anyhow!("Password must be at least 6 characters.");
        let api_err = ApiError::from_anyhow(err);
        assert!(matches!(api_err, ApiError::BadRequest(_)));
    }

    #[test]
    fn test_error_categorization_bad_request_workspace() {
        let err = anyhow::anyhow!("workspace path does not exist: /foo/bar");
        let api_err = ApiError::from_anyhow(err);
        assert!(matches!(api_err, ApiError::BadRequest(_)));
    }

    #[test]
    fn test_error_categorization_internal_default() {
        let err = anyhow::anyhow!("Something went wrong");
        let api_err = ApiError::from_anyhow(err);
        assert!(matches!(api_err, ApiError::Internal(_)));
    }

    #[test]
    fn test_error_response_status_codes() {
        assert_eq!(ApiError::not_found("").status_code(), StatusCode::NOT_FOUND);
        assert_eq!(ApiError::bad_request("").status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(ApiError::unauthorized("").status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(ApiError::forbidden("").status_code(), StatusCode::FORBIDDEN);
        assert_eq!(ApiError::conflict("").status_code(), StatusCode::CONFLICT);
        assert_eq!(ApiError::service_unavailable("").status_code(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(ApiError::internal("").status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(ApiError::bad_gateway("").status_code(), StatusCode::BAD_GATEWAY);
    }
}
