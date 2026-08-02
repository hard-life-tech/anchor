//! Process configuration loaded from environment.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Clone)]
pub struct Config {
    pub github_token: String,
    pub github_user: String,
    /// REST API base, e.g. `https://api.github.com` or `https://ghe.example/api/v3`.
    pub github_api_url: String,
    /// Hostname for clone/display, e.g. `github.com` or `ghe.example`.
    pub github_host: String,
    pub projects_dir: PathBuf,
    pub tmux_session: String,
    pub cursor_cmd: String,
    pub opencode_cmd: String,
    pub port: u16,
    pub log_level: String,
    /// Dashboard login user (default `admin`).
    pub anchor_user: String,
    /// Dashboard login password (required).
    pub anchor_password: String,
    /// HMAC key for session cookies.
    pub session_secret: Vec<u8>,
    /// Set Secure flag on session cookie (HTTPS / Tailscale Serve).
    pub cookie_secure: bool,
    /// SQLite database path.
    pub database_path: PathBuf,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("github_token", &"[redacted]")
            .field("github_user", &self.github_user)
            .field("github_api_url", &self.github_api_url)
            .field("github_host", &self.github_host)
            .field("projects_dir", &self.projects_dir)
            .field("tmux_session", &self.tmux_session)
            .field("cursor_cmd", &self.cursor_cmd)
            .field("opencode_cmd", &self.opencode_cmd)
            .field("port", &self.port)
            .field("log_level", &self.log_level)
            .field("anchor_user", &self.anchor_user)
            .field("anchor_password", &"[redacted]")
            .field("session_secret", &"[redacted]")
            .field("cookie_secure", &self.cookie_secure)
            .field("database_path", &self.database_path)
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    Missing(&'static str),
    #[error("invalid value for {key}: {message}")]
    Invalid { key: &'static str, message: String },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let map: HashMap<String, String> = env::vars().collect();
        Self::from_env_map(&map)
    }

    /// Testable loader: only reads from the provided map (plus `$HOME` for defaults).
    pub fn from_env_map(map: &HashMap<String, String>) -> Result<Self, ConfigError> {
        let github_token = required(map, "GITHUB_TOKEN")?;
        let github_user = required(map, "GITHUB_USER")?;
        let (github_api_url, github_host) = resolve_github_endpoints(map)?;

        let home = map
            .get("HOME")
            .cloned()
            .or_else(|| env::var("HOME").ok())
            .unwrap_or_else(|| "/home/agent".into());

        let projects_dir = map
            .get("PROJECTS_DIR")
            .cloned()
            .unwrap_or_else(|| format!("{home}/projects"));

        let tmux_session = map
            .get("TMUX_SESSION")
            .cloned()
            .unwrap_or_else(|| "agents".into());
        let cursor_cmd = map
            .get("CURSOR_CMD")
            .cloned()
            .unwrap_or_else(|| "cursor-agent".into());
        let opencode_cmd = map
            .get("OPENCODE_CMD")
            .cloned()
            .unwrap_or_else(|| "opencode".into());
        let log_level = map
            .get("LOG_LEVEL")
            .cloned()
            .unwrap_or_else(|| "info".into());

        let port = match map.get("PORT") {
            Some(s) => s.parse::<u16>().map_err(|_| ConfigError::Invalid {
                key: "PORT",
                message: format!("expected u16, got {s}"),
            })?,
            None => 8080,
        };

        let anchor_password = required(map, "ANCHOR_PASSWORD")?;
        let anchor_user = map
            .get("ANCHOR_USER")
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "admin".into());

        let session_secret = match map.get("ANCHOR_SESSION_SECRET") {
            Some(s) if !s.is_empty() => s.as_bytes().to_vec(),
            _ => derive_session_secret(&anchor_password),
        };

        let cookie_secure = map
            .get("ANCHOR_COOKIE_SECURE")
            .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let database_path = resolve_database_path(map, &home)?;

        Ok(Self {
            github_token,
            github_user,
            github_api_url,
            github_host,
            projects_dir: PathBuf::from(projects_dir),
            tmux_session,
            cursor_cmd,
            opencode_cmd,
            port,
            log_level,
            anchor_user,
            anchor_password,
            session_secret,
            cookie_secure,
            database_path,
        })
    }
}

fn derive_session_secret(password: &str) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"anchor-session-v1:");
    hasher.update(password.as_bytes());
    hasher.finalize().to_vec()
}

/// Accept `DATABASE_URL=sqlite:/path` / `sqlite:///path` or `ANCHOR_DB` / default under home.
fn resolve_database_path(
    map: &HashMap<String, String>,
    home: &str,
) -> Result<PathBuf, ConfigError> {
    if let Some(raw) = map.get("DATABASE_URL").filter(|s| !s.is_empty()) {
        return parse_sqlite_url(raw);
    }
    if let Some(p) = map.get("ANCHOR_DB").filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    Ok(PathBuf::from(format!("{home}/projects/anchor.db")))
}

fn parse_sqlite_url(raw: &str) -> Result<PathBuf, ConfigError> {
    let trimmed = raw.trim();
    let path = if let Some(rest) = trimmed.strip_prefix("sqlite:") {
        // sqlite:/abs or sqlite:///abs or sqlite://relative
        let rest = rest.trim_start_matches("//");
        if rest.is_empty() {
            return Err(ConfigError::Invalid {
                key: "DATABASE_URL",
                message: "sqlite path is empty".into(),
            });
        }
        // sqlite:///home/... → /home/... after stripping one leading slash pair carefully
        if trimmed.starts_with("sqlite:///") {
            format!("/{}", rest.trim_start_matches('/'))
        } else if rest.starts_with('/') {
            rest.to_string()
        } else {
            rest.to_string()
        }
    } else if trimmed.starts_with('/') || trimmed.starts_with('.') {
        trimmed.to_string()
    } else {
        return Err(ConfigError::Invalid {
            key: "DATABASE_URL",
            message: format!("expected sqlite:/path or absolute path, got {trimmed}"),
        });
    };
    Ok(PathBuf::from(path))
}

/// Resolve API base + clone/display host for github.com or GitHub Enterprise Server.
pub fn resolve_github_endpoints(
    map: &HashMap<String, String>,
) -> Result<(String, String), ConfigError> {
    let host = map
        .get("GITHUB_HOST")
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "github.com".into());

    if host.contains("://") || host.contains('/') {
        return Err(ConfigError::Invalid {
            key: "GITHUB_HOST",
            message: format!("expected hostname only (e.g. github.com), got {host}"),
        });
    }

    let api = match map.get("GITHUB_API_URL") {
        Some(raw) => {
            let u = raw.trim().trim_end_matches('/').to_string();
            if u.is_empty() {
                return Err(ConfigError::Invalid {
                    key: "GITHUB_API_URL",
                    message: "must not be empty when set".into(),
                });
            }
            if !(u.starts_with("https://") || u.starts_with("http://")) {
                return Err(ConfigError::Invalid {
                    key: "GITHUB_API_URL",
                    message: format!("expected http(s) URL, got {u}"),
                });
            }
            u
        }
        None if host == "github.com" => "https://api.github.com".into(),
        None => format!("https://{host}/api/v3"),
    };

    Ok((api, host))
}

fn required(map: &HashMap<String, String>, key: &'static str) -> Result<String, ConfigError> {
    map.get(key)
        .filter(|v| !v.is_empty())
        .cloned()
        .ok_or(ConfigError::Missing(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn config_requires_token() {
        let err = Config::from_env_map(&map(&[("GITHUB_USER", "x")])).unwrap_err();
        assert!(err.to_string().contains("GITHUB_TOKEN"));
    }

    #[test]
    fn config_requires_user() {
        let err = Config::from_env_map(&map(&[("GITHUB_TOKEN", "t")])).unwrap_err();
        assert!(err.to_string().contains("GITHUB_USER"));
    }

    fn base(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        let mut m = map(&[
            ("GITHUB_TOKEN", "t"),
            ("GITHUB_USER", "u"),
            ("ANCHOR_PASSWORD", "pass"),
            ("HOME", "/tmp/agent"),
        ]);
        for (k, v) in pairs {
            m.insert((*k).to_string(), (*v).to_string());
        }
        m
    }

    #[test]
    fn config_requires_password() {
        let err = Config::from_env_map(&map(&[
            ("GITHUB_TOKEN", "t"),
            ("GITHUB_USER", "u"),
            ("HOME", "/tmp"),
        ]))
        .unwrap_err();
        assert!(err.to_string().contains("ANCHOR_PASSWORD"));
    }

    #[test]
    fn config_defaults() {
        let cfg = Config::from_env_map(&base(&[])).unwrap();
        assert_eq!(cfg.projects_dir, PathBuf::from("/tmp/agent/projects"));
        assert_eq!(cfg.tmux_session, "agents");
        assert_eq!(cfg.cursor_cmd, "cursor-agent");
        assert_eq!(cfg.opencode_cmd, "opencode");
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.log_level, "info");
        assert_eq!(cfg.github_api_url, "https://api.github.com");
        assert_eq!(cfg.github_host, "github.com");
        assert_eq!(cfg.anchor_user, "admin");
        assert_eq!(
            cfg.database_path,
            PathBuf::from("/tmp/agent/projects/anchor.db")
        );
    }

    #[test]
    fn config_database_url_sqlite() {
        let cfg = Config::from_env_map(&base(&[(
            "DATABASE_URL",
            "sqlite:///home/agent/projects/anchor.db",
        )]))
        .unwrap();
        assert_eq!(
            cfg.database_path,
            PathBuf::from("/home/agent/projects/anchor.db")
        );
    }

    #[test]
    fn config_ghes_defaults_api_from_host() {
        let cfg = Config::from_env_map(&base(&[("GITHUB_HOST", "github.example.com")])).unwrap();
        assert_eq!(cfg.github_host, "github.example.com");
        assert_eq!(cfg.github_api_url, "https://github.example.com/api/v3");
    }

    #[test]
    fn config_ghes_explicit_api_url() {
        let cfg = Config::from_env_map(&base(&[
            ("GITHUB_HOST", "github.example.com"),
            ("GITHUB_API_URL", "https://github.example.com/api/v3/"),
        ]))
        .unwrap();
        assert_eq!(cfg.github_api_url, "https://github.example.com/api/v3");
    }

    #[test]
    fn config_rejects_bad_host() {
        let err = Config::from_env_map(&base(&[(
            "GITHUB_HOST",
            "https://github.example.com",
        )]))
        .unwrap_err();
        assert!(err.to_string().contains("GITHUB_HOST"));
    }

    #[test]
    fn config_debug_redacts_secrets() {
        let cfg = Config::from_env_map(&map(&[
            ("GITHUB_TOKEN", "super-secret"),
            ("GITHUB_USER", "u"),
            ("ANCHOR_PASSWORD", "login-secret"),
            ("HOME", "/tmp"),
        ]))
        .unwrap();
        let dbg = format!("{cfg:?}");
        assert!(!dbg.contains("super-secret"));
        assert!(!dbg.contains("login-secret"));
        assert!(dbg.contains("[redacted]"));
    }
}
