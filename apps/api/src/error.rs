//! API error types

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

/// API errors
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Authentication failed: {0}")]
    AuthError(String),
    
    #[error("Authorization failed: {0}")]
    Forbidden(String),
    
    #[error("Resource not found: {0}")]
    NotFound(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Database error: {0}")]
    DatabaseError(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
    
    #[error("Bad request: {0}")]
    BadRequest(String),
}

/// Error response body
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_type, public_message) = match &self {
            ApiError::AuthError(msg) => (StatusCode::UNAUTHORIZED, "auth_error", msg.clone()),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, "forbidden", msg.clone()),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg.clone()),
            ApiError::ValidationError(msg) => (StatusCode::BAD_REQUEST, "validation_error", msg.clone()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone()),
            // Do NOT leak internal database errors or stack traces to the client
            ApiError::DatabaseError(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "A database error occurred".to_string(),
            ),
            ApiError::InternalError(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "An internal server error occurred".to_string(),
            ),
        };

        // Log the full error detail server-side only
        match &self {
            ApiError::DatabaseError(e) | ApiError::InternalError(e) => {
                tracing::error!(error = %e, kind = error_type, "Internal error");
            }
            _ => {}
        }

        let body = ErrorResponse {
            error: error_type.to_string(),
            message: public_message,
            details: None,
        };

        (status, Json(body)).into_response()
    }
}

// Convenience conversions

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::RowNotFound => ApiError::NotFound("Record not found".to_string()),
            _ => ApiError::DatabaseError(e.to_string()),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError::InternalError(e.to_string())
    }
}

impl From<validator::ValidationErrors> for ApiError {
    fn from(e: validator::ValidationErrors) -> Self {
        ApiError::ValidationError(e.to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for ApiError {
    fn from(e: jsonwebtoken::errors::Error) -> Self {
        ApiError::AuthError(e.to_string())
    }
}
