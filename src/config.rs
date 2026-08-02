//! Process configuration loaded from environment.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Clone)]
pub struct Config {
    pub github_token: String,
    pub github_user: String,
    pub projects_dir: PathBuf,
    pub tmux_session: String,
    pub cursor_cmd: String,
    pub opencode_cmd: String,
    pub port: u16,
    pub log_level: String,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("github_token", &"[redacted]")
            .field("github_user", &self.github_user)
            .field("projects_dir", &self.projects_dir)
            .field("tmux_session", &self.tmux_session)
            .field("cursor_cmd", &self.cursor_cmd)
            .field("opencode_cmd", &self.opencode_cmd)
            .field("port", &self.port)
            .field("log_level", &self.log_level)
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

        Ok(Self {
            github_token,
            github_user,
            projects_dir: PathBuf::from(projects_dir),
            tmux_session,
            cursor_cmd,
            opencode_cmd,
            port,
            log_level,
        })
    }
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

    #[test]
    fn config_defaults() {
        let cfg = Config::from_env_map(&map(&[
            ("GITHUB_TOKEN", "t"),
            ("GITHUB_USER", "u"),
            ("HOME", "/tmp/agent"),
        ]))
        .unwrap();
        assert_eq!(cfg.projects_dir, PathBuf::from("/tmp/agent/projects"));
        assert_eq!(cfg.tmux_session, "agents");
        assert_eq!(cfg.cursor_cmd, "cursor-agent");
        assert_eq!(cfg.opencode_cmd, "opencode");
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.log_level, "info");
    }

    #[test]
    fn config_debug_redacts_token() {
        let cfg = Config::from_env_map(&map(&[
            ("GITHUB_TOKEN", "super-secret"),
            ("GITHUB_USER", "u"),
            ("HOME", "/tmp"),
        ]))
        .unwrap();
        let dbg = format!("{cfg:?}");
        assert!(!dbg.contains("super-secret"));
        assert!(dbg.contains("[redacted]"));
    }
}
