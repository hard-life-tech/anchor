//! Session cookie auth. All routes except `/healthz`, `/login`, and static assets.

use std::time::{SystemTime, UNIX_EPOCH};

use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{Form, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use cookie::{Cookie, SameSite};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::error::AppError;
use crate::AppState;

const COOKIE_NAME: &str = "anchor_session";
const SESSION_TTL_SECS: u64 = 60 * 60 * 24 * 14; // 14 days

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct AuthConfig {
    pub username: String,
    pub password: String,
    pub session_secret: Vec<u8>,
    pub cookie_secure: bool,
}

impl AuthConfig {
    pub fn from_config(cfg: &crate::config::Config) -> Self {
        Self {
            username: cfg.anchor_user.clone(),
            password: cfg.anchor_password.clone(),
            session_secret: cfg.session_secret.clone(),
            cookie_secure: cfg.cookie_secure,
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_page).post(login_submit))
        .route("/logout", post(logout))
}

#[derive(Template, WebTemplate)]
#[template(path = "login.html")]
struct LoginTemplate {
    error: Option<String>,
    username: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

async fn login_page(State(state): State<AppState>) -> impl IntoResponse {
    LoginTemplate {
        error: None,
        username: state.auth.username.clone(),
    }
}

async fn login_submit(
    State(state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> Response {
    let user_ok = ct_eq_str(&form.username, &state.auth.username);
    let pass_ok = ct_eq_str(&form.password, &state.auth.password);
    if !(user_ok && pass_ok) {
        tracing::warn!("login failed for user={}", form.username);
        return (
            StatusCode::UNAUTHORIZED,
            LoginTemplate {
                error: Some("Invalid username or password".into()),
                username: form.username,
            },
        )
            .into_response();
    }

    let token = match mint_session(&state.auth, &form.username) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("session mint failed: {e}");
            return AppError::Other(anyhow::anyhow!("session mint failed")).into_response();
        }
    };

    let mut cookie = Cookie::build((COOKIE_NAME, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(cookie::time::Duration::seconds(SESSION_TTL_SECS as i64))
        .build();
    if state.auth.cookie_secure {
        cookie.set_secure(true);
    }

    (
        StatusCode::SEE_OTHER,
        [(header::SET_COOKIE, cookie.to_string()), (header::LOCATION, "/".into())],
    )
        .into_response()
}

async fn logout(State(state): State<AppState>) -> Response {
    let mut cookie = Cookie::build((COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(cookie::time::Duration::seconds(0))
        .build();
    if state.auth.cookie_secure {
        cookie.set_secure(true);
    }
    (
        StatusCode::SEE_OTHER,
        [(header::SET_COOKIE, cookie.to_string()), (header::LOCATION, "/login".into())],
    )
        .into_response()
}

/// Middleware: require a valid session cookie except for public paths.
pub async fn require_auth(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path();
    if is_public(path) {
        return next.run(req).await;
    }

    let headers = req.headers();
    if session_valid(&state.auth, headers) {
        return next.run(req).await;
    }

    unauthorized_response(headers, path)
}

fn is_public(path: &str) -> bool {
    path == "/healthz"
        || path == "/login"
        || path == "/static/style.css"
        || path.starts_with("/static/")
}

fn unauthorized_response(headers: &HeaderMap, path: &str) -> Response {
    let wants_html = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|a| a.contains("text/html"))
        .unwrap_or(false);
    let is_api = path.starts_with("/api/") || path.starts_with("/ws/");
    if wants_html && !is_api {
        return Redirect::temporary("/login").into_response();
    }
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(crate::error::ErrorBody {
            error: "authentication required".into(),
            code: "UNAUTHORIZED".into(),
        }),
    )
        .into_response()
}

pub fn session_valid(auth: &AuthConfig, headers: &HeaderMap) -> bool {
    let Some(raw) = cookie_value(headers, COOKIE_NAME) else {
        return false;
    };
    verify_session(auth, &raw).is_ok()
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie_header.split(';') {
        let part = part.trim();
        if let Some((k, v)) = part.split_once('=') {
            if k == name {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn mint_session(auth: &AuthConfig, username: &str) -> anyhow::Result<String> {
    let exp = now_secs() + SESSION_TTL_SECS;
    let payload = format!("{username}|{exp}");
    let sig = sign(auth, payload.as_bytes())?;
    Ok(format!("{payload}|{sig}"))
}

fn verify_session(auth: &AuthConfig, token: &str) -> anyhow::Result<String> {
    let mut parts = token.rsplitn(2, '|');
    let sig = parts.next().ok_or_else(|| anyhow::anyhow!("bad token"))?;
    let payload = parts.next().ok_or_else(|| anyhow::anyhow!("bad token"))?;
    let expected = sign(auth, payload.as_bytes())?;
    if !bool::from(expected.as_bytes().ct_eq(sig.as_bytes())) {
        anyhow::bail!("bad signature");
    }
    let (user, exp_s) = payload
        .split_once('|')
        .ok_or_else(|| anyhow::anyhow!("bad payload"))?;
    let exp: u64 = exp_s.parse()?;
    if now_secs() > exp {
        anyhow::bail!("expired");
    }
    if !ct_eq_str(user, &auth.username) {
        anyhow::bail!("user mismatch");
    }
    Ok(user.to_string())
}

fn sign(auth: &AuthConfig, data: &[u8]) -> anyhow::Result<String> {
    let mut mac = HmacSha256::new_from_slice(&auth.session_secret)
        .map_err(|e| anyhow::anyhow!("hmac key: {e}"))?;
    mac.update(data);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn ct_eq_str(a: &str, b: &str) -> bool {
    // Pad to equal length to avoid length leak on compare; still leaks length via padding.
    let max = a.len().max(b.len());
    let mut aa = vec![0u8; max];
    let mut bb = vec![0u8; max];
    aa[..a.len()].copy_from_slice(a.as_bytes());
    bb[..b.len()].copy_from_slice(b.as_bytes());
    bool::from(aa.ct_eq(&bb)) && a.len() == b.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn test_auth() -> AuthConfig {
        AuthConfig {
            username: "admin".into(),
            password: "secret".into(),
            session_secret: b"test-session-secret-bytes!!".to_vec(),
            cookie_secure: false,
        }
    }

    #[test]
    fn mint_and_verify_roundtrip() {
        let auth = test_auth();
        let tok = mint_session(&auth, "admin").unwrap();
        assert_eq!(verify_session(&auth, &tok).unwrap(), "admin");
    }

    #[test]
    fn rejects_tampered_token() {
        let auth = test_auth();
        let tok = mint_session(&auth, "admin").unwrap();
        let bad = format!("{tok}x");
        assert!(verify_session(&auth, &bad).is_err());
    }

    #[test]
    fn password_compare_constant_ish() {
        assert!(ct_eq_str("abc", "abc"));
        assert!(!ct_eq_str("abc", "abd"));
        assert!(!ct_eq_str("abc", "ab"));
    }

    #[test]
    fn public_paths() {
        assert!(is_public("/healthz"));
        assert!(is_public("/login"));
        assert!(is_public("/static/style.css"));
        assert!(!is_public("/"));
        assert!(!is_public("/api/repos"));
        assert!(!is_public("/ws/terminal/x/cursor"));
    }

    #[tokio::test]
    async fn middleware_rejects_api_without_cookie() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let auth = Arc::new(test_auth());
        // Minimal state via a tiny router that only runs middleware + a stub.
        async fn ok() -> &'static str {
            "secret"
        }
        let app = Router::new()
            .route("/api/repos", get(ok))
            .layer(axum::middleware::from_fn({
                let auth = Arc::clone(&auth);
                move |req: Request<Body>, next: Next| {
                    let auth = Arc::clone(&auth);
                    async move {
                        let path = req.uri().path().to_string();
                        if is_public(&path) {
                            return next.run(req).await;
                        }
                        if session_valid(&auth, req.headers()) {
                            return next.run(req).await;
                        }
                        unauthorized_response(req.headers(), &path)
                    }
                }
            }));

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/repos")
                    .header(header::ACCEPT, "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn middleware_allows_valid_session() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let auth = test_auth();
        let tok = mint_session(&auth, "admin").unwrap();
        let auth = Arc::new(auth);

        async fn ok() -> &'static str {
            "secret"
        }
        let app = Router::new()
            .route("/api/repos", get(ok))
            .layer(axum::middleware::from_fn({
                let auth = Arc::clone(&auth);
                move |req: Request<Body>, next: Next| {
                    let auth = Arc::clone(&auth);
                    async move {
                        let path = req.uri().path().to_string();
                        if session_valid(&auth, req.headers()) {
                            return next.run(req).await;
                        }
                        unauthorized_response(req.headers(), &path)
                    }
                }
            }));

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/repos")
                    .header(header::COOKIE, format!("{COOKIE_NAME}={tok}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
