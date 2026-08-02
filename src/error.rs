//! Shared error and API error JSON shape.

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use thiserror::Error;
use tracing_subscriber::fmt::MakeWriter;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
    pub code: String,
}

/// Best-effort redaction so git/API stderr / logs never echo PATs.
pub fn redact_secrets(s: &str) -> String {
    let mut out = s.to_string();
    out = redact_env_assignment(&out, "GITHUB_TOKEN");
    out = redact_env_assignment(&out, "ANCHOR_PASSWORD");
    out = redact_env_assignment(&out, "ANCHOR_SESSION_SECRET");
    out = redact_prefix_token(&out, "github_pat_");
    out = redact_prefix_token(&out, "ghp_");
    out = redact_prefix_token(&out, "gho_");
    out = redact_prefix_token(&out, "ghu_");
    out = redact_prefix_token(&out, "ghs_");
    out = redact_prefix_token(&out, "ghr_");
    out = redact_bearer(&out);
    out
}

/// Also scrub an exact known secret value (opaque tokens without a PAT prefix).
pub fn redact_secrets_with_known(s: &str, known: &str) -> String {
    let mut out = redact_secrets(s);
    if !known.is_empty() {
        out = out.replace(known, "[redacted]");
    }
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

fn redact_env_assignment(s: &str, key: &str) -> String {
    // Match KEY=value / KEY: value (value = non-whitespace run).
    let patterns = [format!("{key}="), format!("{key}:")];
    let mut out = s.to_string();
    for pat in &patterns {
        let mut rebuilt = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(idx) = rest.find(pat.as_str()) {
            rebuilt.push_str(&rest[..idx]);
            rebuilt.push_str(pat);
            let after = &rest[idx + pat.len()..];
            let after = after.trim_start_matches([' ', '\t']);
            let end = after
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                .unwrap_or(after.len());
            rebuilt.push_str("[redacted]");
            rest = &after[end..];
        }
        rebuilt.push_str(rest);
        out = rebuilt;
    }
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

/// `MakeWriter` that runs every tracing line through [`redact_secrets`].
#[derive(Clone, Default, Debug)]
pub struct RedactingMakeWriter {
    inner: Option<Arc<Mutex<Vec<u8>>>>,
}

impl RedactingMakeWriter {
    pub fn stdout() -> Self {
        Self { inner: None }
    }

    /// Capture redacted output into a shared buffer (tests).
    #[cfg(test)]
    pub fn buffer(buf: Arc<Mutex<Vec<u8>>>) -> Self {
        Self { inner: Some(buf) }
    }
}

pub struct RedactingWriter {
    target: RedactTarget,
}

enum RedactTarget {
    Stdout(io::Stdout),
    Buffer(Arc<Mutex<Vec<u8>>>),
}

impl Write for RedactingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let raw = String::from_utf8_lossy(buf);
        let redacted = redact_secrets(&raw);
        match &mut self.target {
            RedactTarget::Stdout(out) => {
                out.write_all(redacted.as_bytes())?;
            }
            RedactTarget::Buffer(buf) => {
                buf.lock()
                    .map_err(|_| io::Error::other("redacting buffer poisoned"))?
                    .extend_from_slice(redacted.as_bytes());
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.target {
            RedactTarget::Stdout(out) => out.flush(),
            RedactTarget::Buffer(_) => Ok(()),
        }
    }
}

impl<'a> MakeWriter<'a> for RedactingMakeWriter {
    type Writer = RedactingWriter;

    fn make_writer(&'a self) -> Self::Writer {
        let target = match &self.inner {
            Some(buf) => RedactTarget::Buffer(Arc::clone(buf)),
            None => RedactTarget::Stdout(io::stdout()),
        };
        RedactingWriter { target }
    }
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

    /// Safe operator-facing message (PAT shapes stripped).
    pub fn safe_message(&self) -> String {
        redact_secrets(&self.to_string())
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
            error: self.safe_message(),
            code: self.code().into(),
        };
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::response::IntoResponse;

    #[test]
    fn redacts_pat_shapes() {
        let s = "clone failed: Authorization: Bearer github_pat_ABC123xyz and ghp_secret99";
        let r = redact_secrets(s);
        assert!(!r.contains("ABC123"));
        assert!(!r.contains("secret99"));
        assert!(r.contains("[redacted]"));
    }

    #[test]
    fn redacts_env_assignment() {
        let s = "env dump GITHUB_TOKEN=github_pat_LEAKED99 rest";
        let r = redact_secrets(s);
        assert!(!r.contains("LEAKED99"));
        assert!(r.contains("GITHUB_TOKEN=[redacted]"));
    }

    #[test]
    fn redacts_known_opaque_token() {
        let s = "failed with opaque-secret-value in stderr";
        let r = redact_secrets_with_known(s, "opaque-secret-value");
        assert!(!r.contains("opaque-secret-value"));
        assert!(r.contains("[redacted]"));
    }

    #[test]
    fn leaves_clean_errors_alone() {
        let s = "project not on disk: demo";
        assert_eq!(redact_secrets(s), s);
    }

    #[tokio::test]
    async fn api_error_json_never_contains_pat() {
        let err = AppError::BadGateway(
            "git clone failed: Authorization: Bearer ghp_SuperSecretToken99".into(),
        );
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
        let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(!body.contains("SuperSecretToken99"));
        assert!(!body.contains("ghp_"));
        assert!(body.contains("[redacted]"));
        assert!(body.contains("BAD_GATEWAY"));
    }

    #[test]
    fn tracing_writer_redacts_pat_shapes() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let writer = RedactingMakeWriter::buffer(Arc::clone(&buf));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(writer)
            .with_level(false)
            .with_target(false)
            .with_ansi(false)
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            tracing::error!("fetch failed Authorization: Bearer ghp_TraceLeakToken1");
        });

        let logged = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(
            !logged.contains("TraceLeakToken1"),
            "token leaked into tracing output: {logged}"
        );
        assert!(logged.contains("[redacted]"));
    }
}
