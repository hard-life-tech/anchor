//! SQLite settings store + project membership tables.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

const MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

const MIGRATION_V2: &str = r#"
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    default_branch TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS project_repos (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    owner TEXT NOT NULL,
    name TEXT NOT NULL,
    full_name TEXT NOT NULL,
    clone_url TEXT NOT NULL,
    private INTEGER NOT NULL DEFAULT 0,
    default_branch TEXT NOT NULL DEFAULT 'main',
    sort INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (project_id, owner, name)
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
        conn.execute_batch(MIGRATION_V2)?;
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

    pub fn insert_project(&self, project: &ProjectRecord) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        conn.execute(
            "INSERT INTO projects (id, slug, name, default_branch, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                project.id,
                project.slug,
                project.name,
                project.default_branch,
                project.created_at,
            ],
        )?;
        for (i, repo) in project.repos.iter().enumerate() {
            conn.execute(
                "INSERT INTO project_repos
                 (project_id, owner, name, full_name, clone_url, private, default_branch, sort)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    project.id,
                    repo.owner,
                    repo.name,
                    repo.full_name,
                    repo.clone_url,
                    if repo.private { 1 } else { 0 },
                    repo.default_branch,
                    i as i64,
                ],
            )?;
        }
        Ok(())
    }

    pub fn update_project_meta(
        &self,
        slug: &str,
        name: Option<&str>,
        default_branch: Option<&str>,
    ) -> Result<bool> {
        let conn = self.conn.lock().expect("db lock");
        let existing: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT name, default_branch FROM projects WHERE slug = ?1",
                params![slug],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((cur_name, cur_branch)) = existing else {
            return Ok(false);
        };
        let new_name = name.unwrap_or(&cur_name);
        let new_branch = default_branch
            .map(|s| Some(s.to_string()))
            .unwrap_or(cur_branch);
        conn.execute(
            "UPDATE projects SET name = ?1, default_branch = ?2 WHERE slug = ?3",
            params![new_name, new_branch, slug],
        )?;
        Ok(true)
    }

    pub fn delete_project(&self, slug: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("db lock");
        let n = conn.execute("DELETE FROM projects WHERE slug = ?1", params![slug])?;
        Ok(n > 0)
    }

    pub fn get_project_by_slug(&self, slug: &str) -> Result<Option<ProjectRecord>> {
        let conn = self.conn.lock().expect("db lock");
        let row: Option<(String, String, String, Option<String>, String)> = conn
            .query_row(
                "SELECT id, slug, name, default_branch, created_at FROM projects WHERE slug = ?1",
                params![slug],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()?;
        let Some((id, slug, name, default_branch, created_at)) = row else {
            return Ok(None);
        };
        let repos = load_repos(&conn, &id)?;
        Ok(Some(ProjectRecord {
            id,
            slug,
            name,
            default_branch,
            created_at,
            repos,
        }))
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectRecord>> {
        let conn = self.conn.lock().expect("db lock");
        let mut stmt = conn.prepare(
            "SELECT id, slug, name, default_branch, created_at FROM projects ORDER BY slug",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, slug, name, default_branch, created_at) = row?;
            let repos = load_repos(&conn, &id)?;
            out.push(ProjectRecord {
                id,
                slug,
                name,
                default_branch,
                created_at,
                repos,
            });
        }
        Ok(out)
    }

    pub fn add_repo(&self, project_id: &str, repo: &ProjectRepoRecord) -> Result<()> {
        let conn = self.conn.lock().expect("db lock");
        let sort: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort), -1) + 1 FROM project_repos WHERE project_id = ?1",
                params![project_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        conn.execute(
            "INSERT INTO project_repos
             (project_id, owner, name, full_name, clone_url, private, default_branch, sort)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(project_id, owner, name) DO UPDATE SET
               full_name = excluded.full_name,
               clone_url = excluded.clone_url,
               private = excluded.private,
               default_branch = excluded.default_branch",
            params![
                project_id,
                repo.owner,
                repo.name,
                repo.full_name,
                repo.clone_url,
                if repo.private { 1 } else { 0 },
                repo.default_branch,
                sort,
            ],
        )?;
        Ok(())
    }

    pub fn remove_repo(&self, project_id: &str, owner: &str, name: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("db lock");
        let n = conn.execute(
            "DELETE FROM project_repos WHERE project_id = ?1 AND owner = ?2 AND name = ?3",
            params![project_id, owner, name],
        )?;
        Ok(n > 0)
    }
}

fn load_repos(conn: &Connection, project_id: &str) -> Result<Vec<ProjectRepoRecord>> {
    let mut stmt = conn.prepare(
        "SELECT owner, name, full_name, clone_url, private, default_branch
         FROM project_repos WHERE project_id = ?1 ORDER BY sort, full_name",
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok(ProjectRepoRecord {
            owner: row.get(0)?,
            name: row.get(1)?,
            full_name: row.get(2)?,
            clone_url: row.get(3)?,
            private: row.get::<_, i64>(4)? != 0,
            default_branch: row.get(5)?,
        })
    })?;
    let mut repos = Vec::new();
    for row in rows {
        repos.push(row?);
    }
    Ok(repos)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectRepoRecord {
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub clone_url: String,
    pub private: bool,
    pub default_branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectRecord {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub default_branch: Option<String>,
    pub created_at: String,
    pub repos: Vec<ProjectRepoRecord>,
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

    #[test]
    fn projects_crud() {
        let tmp = TempDir::new().unwrap();
        let db = Db::open(tmp.path().join("anchor.db")).unwrap();
        let project = ProjectRecord {
            id: "p1".into(),
            slug: "platform".into(),
            name: "Platform".into(),
            default_branch: Some("main".into()),
            created_at: "2026-08-02T00:00:00Z".into(),
            repos: vec![ProjectRepoRecord {
                owner: "alice".into(),
                name: "anchor".into(),
                full_name: "alice/anchor".into(),
                clone_url: "https://github.com/alice/anchor.git".into(),
                private: true,
                default_branch: "main".into(),
            }],
        };
        db.insert_project(&project).unwrap();
        let loaded = db.get_project_by_slug("platform").unwrap().unwrap();
        assert_eq!(loaded.name, "Platform");
        assert_eq!(loaded.repos.len(), 1);
        assert_eq!(loaded.repos[0].full_name, "alice/anchor");

        db.add_repo(
            "p1",
            &ProjectRepoRecord {
                owner: "alice".into(),
                name: "docs".into(),
                full_name: "alice/docs".into(),
                clone_url: "https://github.com/alice/docs.git".into(),
                private: false,
                default_branch: "main".into(),
            },
        )
        .unwrap();
        let loaded = db.get_project_by_slug("platform").unwrap().unwrap();
        assert_eq!(loaded.repos.len(), 2);

        assert!(db.remove_repo("p1", "alice", "docs").unwrap());
        let loaded = db.get_project_by_slug("platform").unwrap().unwrap();
        assert_eq!(loaded.repos.len(), 1);

        assert!(db.update_project_meta("platform", Some("Plat"), None).unwrap());
        assert_eq!(
            db.get_project_by_slug("platform").unwrap().unwrap().name,
            "Plat"
        );
        assert!(db.delete_project("platform").unwrap());
        assert!(db.get_project_by_slug("platform").unwrap().is_none());
    }
}
