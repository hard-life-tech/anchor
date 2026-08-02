//! Shared error and API error JSON shape.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
    pub code: String,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    #[allow(dead_code)]
    NotImplemented(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    BadGateway(String),
    #[error("{0}")]
    #[allow(dead_code)]
    Conflict(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AppError {
    #[allow(dead_code)]
    pub fn not_implemented() -> Self {
        Self::NotImplemented("not implemented".into())
    }

    fn code(&self) -> &'static str {
        match self {
            Self::NotImplemented(_) => "NOT_IMPLEMENTED",
            Self::NotFound(_) => "NOT_FOUND",
            Self::BadGateway(_) => "BAD_GATEWAY",
            Self::Conflict(_) => "CONFLICT",
            Self::Other(_) => "INTERNAL",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadGateway(_) => StatusCode::BAD_GATEWAY,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = ErrorBody {
            error: self.to_string(),
            code: self.code().into(),
        };
        (status, Json(body)).into_response()
    }
}
