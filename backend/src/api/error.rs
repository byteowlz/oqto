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

/// Convert anyhow errors to API errors
impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        let msg = err.to_string();

        // Try to determine the appropriate error type from the message
        if msg.to_lowercase().contains("not found") {
            ApiError::NotFound(msg)
        } else if msg.to_lowercase().contains("unauthorized") || msg.to_lowercase().contains("authentication") {
            ApiError::Unauthorized(msg)
        } else if msg.to_lowercase().contains("forbidden") || msg.to_lowercase().contains("permission") {
            ApiError::Forbidden(msg)
        } else if msg.to_lowercase().contains("conflict") || msg.to_lowercase().contains("already exists") {
            ApiError::Conflict(msg)
        } else if msg.to_lowercase().contains("unavailable") || msg.to_lowercase().contains("connection refused") {
            ApiError::ServiceUnavailable(msg)
        } else {
            ApiError::Internal(msg)
        }
    }
}

/// Result type for API handlers
pub type ApiResult<T> = Result<T, ApiError>;
