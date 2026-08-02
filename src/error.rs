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

/// Best-effort redaction so git/API stderr never echoes PATs back to clients.
pub fn redact_secrets(s: &str) -> String {
    let mut out = s.to_string();
    out = redact_prefix_token(&out, "github_pat_");
    out = redact_prefix_token(&out, "ghp_");
    out = redact_prefix_token(&out, "gho_");
    out = redact_prefix_token(&out, "ghu_");
    out = redact_prefix_token(&out, "ghs_");
    out = redact_prefix_token(&out, "ghr_");
    out = redact_bearer(&out);
    out
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'
}

fn redact_prefix_token(s: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(idx) = rest.find(prefix) {
        out.push_str(&rest[..idx]);
        out.push_str("[redacted]");
        let after = &rest[idx + prefix.len()..];
        let end = after.find(|c: char| !is_token_char(c)).unwrap_or(after.len());
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn redact_bearer(s: &str) -> String {
    // Case-insensitive scan for "bearer " then redact following token chars.
    let lower = s.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    while i < chars.len() {
        let (byte_idx, _) = chars[i];
        if lower[byte_idx..].starts_with("bearer ") {
            out.push_str(&s[byte_idx..byte_idx + "bearer ".len()]);
            // advance past "bearer "
            let target = byte_idx + "bearer ".len();
            while i < chars.len() && chars[i].0 < target {
                i += 1;
            }
            while i < chars.len() && is_token_char(chars[i].1) {
                i += 1;
            }
            out.push_str("[redacted]");
            continue;
        }
        out.push(chars[i].1);
        i += 1;
    }
    out
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
            error: redact_secrets(&self.to_string()),
            code: self.code().into(),
        };
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_pat_shapes() {
        let s = "clone failed: Authorization: Bearer github_pat_ABC123xyz and ghp_secret99";
        let r = redact_secrets(s);
        assert!(!r.contains("ABC123"));
        assert!(!r.contains("secret99"));
        assert!(r.contains("[redacted]"));
    }

    #[test]
    fn leaves_clean_errors_alone() {
        let s = "project not on disk: demo";
        assert_eq!(redact_secrets(s), s);
    }
}
