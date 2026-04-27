use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider request failed: {0}")]
    Request(String),
    #[error("provider rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("provider returned invalid response: {0}")]
    InvalidResponse(String),
    #[error("provider not configured")]
    NotConfigured,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("retrieval weights must sum to 1.0, got {sum}")]
    WeightsMustSumToOne { sum: f32 },
    #[error("invalid config value for {field}: {message}")]
    InvalidValue {
        field: &'static str,
        message: String,
    },
    #[error("failed to initialize telemetry: {0}")]
    Telemetry(String),
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("not found: {resource}")]
    NotFound { resource: String },
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden: insufficient scope")]
    Forbidden,
    #[error("validation error: {0}")]
    Validation(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("rate limited")]
    RateLimited { retry_after_secs: u64 },
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
    request_id: String,
}

impl AppError {
    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            AppError::NotFound { .. } => (StatusCode::NOT_FOUND, "not_found"),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::Validation(_) => (StatusCode::BAD_REQUEST, "validation_error"),
            AppError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            AppError::RateLimited { .. } => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            AppError::Provider(_) => (StatusCode::BAD_GATEWAY, "provider_error"),
            AppError::Database(_) | AppError::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        }
    }

    fn client_message(&self) -> String {
        match self {
            AppError::Database(_) | AppError::Internal(_) => "internal server error".to_owned(),
            AppError::Provider(_) => "provider request failed".to_owned(),
            other => other.to_string(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();

        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = ?self, "internal error");
        }

        let retry_after_secs = match &self {
            AppError::RateLimited { retry_after_secs } => Some(*retry_after_secs),
            _ => None,
        };

        let body = ErrorEnvelope {
            error: ErrorBody {
                code,
                message: self.client_message(),
                request_id: String::new(),
            },
        };

        let mut response = (status, Json(body)).into_response();
        if let Some(seconds) = retry_after_secs {
            if let Ok(value) = HeaderValue::from_str(&seconds.to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_maps_to_404() {
        let response = AppError::NotFound {
            resource: "memory".to_owned(),
        }
        .into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn rate_limited_sets_retry_after_header() {
        let response = AppError::RateLimited {
            retry_after_secs: 30,
        }
        .into_response();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response.headers().get(header::RETRY_AFTER),
            Some(&HeaderValue::from_static("30"))
        );
    }
}
