use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::json;
use thiserror::Error;

use crate::types::{ErrorCode, RuntimeErrorInfo};

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("task not found: {0}")]
    NotFound(String),
    #[error("queue is full")]
    QueueFull,
    #[error("task is already terminal: {0}")]
    Conflict(String),
    #[error("launch failed: {0}")]
    LaunchFailed(String),
    #[error("sandbox setup failed: {0}")]
    SandboxSetup(String),
    #[error("unsupported capability: {0}")]
    UnsupportedCapability(String),
    #[error("insufficient resources: {0}")]
    InsufficientResources(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn code(&self) -> ErrorCode {
        match self {
            AppError::InvalidInput(_) => ErrorCode::InvalidInput,
            AppError::NotFound(_) => ErrorCode::Internal,
            AppError::QueueFull => ErrorCode::ResourceLimitExceeded,
            AppError::Conflict(_) => ErrorCode::Internal,
            AppError::LaunchFailed(_) => ErrorCode::LaunchFailed,
            AppError::SandboxSetup(_) => ErrorCode::SandboxSetupFailed,
            AppError::UnsupportedCapability(_) => ErrorCode::UnsupportedCapability,
            AppError::InsufficientResources(_) => ErrorCode::InsufficientResources,
            AppError::Io(_) | AppError::Sqlite(_) | AppError::Json(_) | AppError::Http(_) => {
                ErrorCode::Internal
            }
            AppError::Internal(_) => ErrorCode::Internal,
        }
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::InvalidInput(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::QueueFull => StatusCode::TOO_MANY_REQUESTS,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::LaunchFailed(_)
            | AppError::SandboxSetup(_)
            | AppError::UnsupportedCapability(_)
            | AppError::InsufficientResources(_) => StatusCode::BAD_REQUEST,
            AppError::Io(_)
            | AppError::Sqlite(_)
            | AppError::Json(_)
            | AppError::Http(_)
            | AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn as_runtime_error(&self) -> RuntimeErrorInfo {
        RuntimeErrorInfo {
            code: self.code(),
            message: self.to_string(),
            details: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    error: RuntimeErrorInfo,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = Json(ErrorEnvelope {
            error: self.as_runtime_error(),
        });
        (status, body).into_response()
    }
}

pub fn json_error(code: ErrorCode, message: impl Into<String>) -> serde_json::Value {
    json!({
        "error": {
            "code": code,
            "message": message.into(),
        }
    })
}
