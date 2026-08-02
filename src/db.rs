//! SQLite settings store (operator prefs + agent command overrides).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

const MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl std::fmt::Debug for Db {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Db").field("path", &self.path).finish()
    }
}

impl Db {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create db dir {}", parent.display()))?;
        }
        let conn = Connection::open(&path)
            .with_context(|| format!("open sqlite {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(MIGRATION_V1)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now')",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn load_agent_settings(&self) -> Result<AgentSettings> {
        Ok(AgentSettings {
            cursor_cmd: self.get("cursor_cmd")?,
            opencode_cmd: self.get("opencode_cmd")?,
            cursor_args: self.get("cursor_args")?.unwrap_or_default(),
            opencode_args: self.get("opencode_args")?.unwrap_or_default(),
            cursor_enabled: self
                .get("cursor_enabled")?
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(true),
            opencode_enabled: self
                .get("opencode_enabled")?
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(true),
            last_opened_project: self.get("last_opened_project")?,
            install_notes: self.get("install_notes")?.unwrap_or_default(),
        })
    }

    pub fn save_agent_settings(&self, s: &AgentSettings) -> Result<()> {
        if let Some(ref v) = s.cursor_cmd {
            self.set("cursor_cmd", v)?;
        } else {
            self.set("cursor_cmd", "")?;
        }
        if let Some(ref v) = s.opencode_cmd {
            self.set("opencode_cmd", v)?;
        } else {
            self.set("opencode_cmd", "")?;
        }
        self.set("cursor_args", &s.cursor_args)?;
        self.set("opencode_args", &s.opencode_args)?;
        self.set(
            "cursor_enabled",
            if s.cursor_enabled { "1" } else { "0" },
        )?;
        self.set(
            "opencode_enabled",
            if s.opencode_enabled { "1" } else { "0" },
        )?;
        if let Some(ref p) = s.last_opened_project {
            self.set("last_opened_project", p)?;
        }
        self.set("install_notes", &s.install_notes)?;
        Ok(())
    }
}

/// Persisted operator settings. Empty command strings mean “use env default”.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettings {
    pub cursor_cmd: Option<String>,
    pub opencode_cmd: Option<String>,
    pub cursor_args: String,
    pub opencode_args: String,
    pub cursor_enabled: bool,
    pub opencode_enabled: bool,
    pub last_opened_project: Option<String>,
    pub install_notes: String,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            cursor_cmd: None,
            opencode_cmd: None,
            cursor_args: String::new(),
            opencode_args: String::new(),
            cursor_enabled: true,
            opencode_enabled: true,
            last_opened_project: None,
            install_notes: String::new(),
        }
    }
}

impl AgentSettings {
    /// Resolve launch command for an agent pane (cmd + optional args).
    pub fn resolve_cmd(&self, agent: &str, env_default: &str) -> String {
        let (cmd, args, enabled) = match agent {
            "cursor" => (
                self.cursor_cmd
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(env_default),
                self.cursor_args.as_str(),
                self.cursor_enabled,
            ),
            "opencode" => (
                self.opencode_cmd
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(env_default),
                self.opencode_args.as_str(),
                self.opencode_enabled,
            ),
            _ => (env_default, "", true),
        };
        if !enabled {
            // Idle shell — operator can launch manually from the terminal.
            return "bash".into();
        }
        if args.trim().is_empty() {
            cmd.to_string()
        } else {
            format!("{cmd} {}", args.trim())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn settings_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let db = Db::open(tmp.path().join("anchor.db")).unwrap();
        let mut s = AgentSettings::default();
        s.cursor_cmd = Some("cursor-agent".into());
        s.cursor_args = "--model gpt".into();
        s.opencode_enabled = false;
        db.save_agent_settings(&s).unwrap();
        let loaded = db.load_agent_settings().unwrap();
        assert_eq!(loaded.cursor_cmd.as_deref(), Some("cursor-agent"));
        assert_eq!(loaded.cursor_args, "--model gpt");
        assert!(!loaded.opencode_enabled);
        assert_eq!(
            loaded.resolve_cmd("cursor", "fallback"),
            "cursor-agent --model gpt"
        );
        assert_eq!(loaded.resolve_cmd("opencode", "opencode"), "bash");
    }
}
