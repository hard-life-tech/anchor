//! Project metadata: SQLite + `.anchor/project.json` mirror.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::db::{Db, ProjectRecord, ProjectRepoRecord};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectJson {
    pub id: String,
    pub slug: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    pub members: Vec<ProjectMemberJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectMemberJson {
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub clone_url: String,
    pub private: bool,
    pub default_branch: String,
}

impl From<&ProjectRecord> for ProjectJson {
    fn from(p: &ProjectRecord) -> Self {
        Self {
            id: p.id.clone(),
            slug: p.slug.clone(),
            name: p.name.clone(),
            default_branch: p.default_branch.clone(),
            members: p
                .repos
                .iter()
                .map(|r| ProjectMemberJson {
                    owner: r.owner.clone(),
                    name: r.name.clone(),
                    full_name: r.full_name.clone(),
                    clone_url: r.clone_url.clone(),
                    private: r.private,
                    default_branch: r.default_branch.clone(),
                })
                .collect(),
        }
    }
}

impl ProjectJson {
    pub fn to_record(&self) -> ProjectRecord {
        ProjectRecord {
            id: self.id.clone(),
            slug: self.slug.clone(),
            name: self.name.clone(),
            default_branch: self.default_branch.clone(),
            created_at: Utc::now().to_rfc3339(),
            repos: self
                .members
                .iter()
                .map(|m| ProjectRepoRecord {
                    owner: m.owner.clone(),
                    name: m.name.clone(),
                    full_name: m.full_name.clone(),
                    clone_url: m.clone_url.clone(),
                    private: m.private,
                    default_branch: m.default_branch.clone(),
                })
                .collect(),
        }
    }
}

pub fn new_project_id() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Derive a URL-safe slug from a display name.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in name.chars() {
        let lower = c.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "project".into()
    } else {
        out
    }
}

pub fn validate_slug(slug: &str) -> Result<(), String> {
    if slug.is_empty() || slug.len() > 64 {
        return Err("slug must be 1–64 characters".into());
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err("slug must be lowercase alphanumeric, hyphen, or underscore".into());
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return Err("slug must not start or end with a hyphen".into());
    }
    Ok(())
}

pub fn project_json_path(project_dir: &Path) -> std::path::PathBuf {
    project_dir.join(".anchor").join("project.json")
}

pub async fn write_project_json(project_dir: &Path, meta: &ProjectJson) -> Result<()> {
    let dir = project_dir.join(".anchor");
    tokio::fs::create_dir_all(&dir)
        .await
        .with_context(|| format!("create {}", dir.display()))?;
    let path = project_json_path(project_dir);
    let body = serde_json::to_string_pretty(meta).context("serialize project.json")?;
    tokio::fs::write(&path, body)
        .await
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

pub async fn read_project_json(project_dir: &Path) -> Result<Option<ProjectJson>> {
    let path = project_json_path(project_dir);
    if !path.exists() {
        return Ok(None);
    }
    let raw = tokio::fs::read_to_string(&path)
        .await
        .with_context(|| format!("read {}", path.display()))?;
    let meta: ProjectJson = serde_json::from_str(&raw).context("parse project.json")?;
    Ok(Some(meta))
}

pub async fn delete_project_json(project_dir: &Path) -> Result<()> {
    let path = project_json_path(project_dir);
    if path.exists() {
        tokio::fs::remove_file(&path)
            .await
            .with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}

/// Persist project to SQLite and mirror to disk.
pub async fn save_project(db: &Db, projects_dir: &Path, project: &ProjectRecord) -> Result<()> {
    // Upsert: delete + insert is simplest when membership may change wholesale.
    let _ = db.delete_project(&project.slug);
    db.insert_project(project)?;
    let dir = projects_dir.join(&project.slug);
    tokio::fs::create_dir_all(&dir)
        .await
        .with_context(|| format!("create {}", dir.display()))?;
    write_project_json(&dir, &ProjectJson::from(project)).await?;
    Ok(())
}

/// Sync disk JSON → DB when DB is missing the project (restart / external edit).
pub async fn ensure_db_has_project(db: &Db, projects_dir: &Path, slug: &str) -> Result<Option<ProjectRecord>> {
    if let Some(p) = db.get_project_by_slug(slug)? {
        return Ok(Some(p));
    }
    let dir = projects_dir.join(slug);
    if let Some(meta) = read_project_json(&dir).await? {
        let record = meta.to_record();
        db.insert_project(&record)?;
        return Ok(Some(record));
    }
    Ok(None)
}

/// Load all projects: prefer SQLite, merge any on-disk JSON not yet in DB.
pub async fn list_all_projects(db: &Db, projects_dir: &Path) -> Result<Vec<ProjectRecord>> {
    let mut projects = db.list_projects()?;
    let mut known: std::collections::HashSet<String> =
        projects.iter().map(|p| p.slug.clone()).collect();

    if projects_dir.exists() {
        let mut entries = tokio::fs::read_dir(projects_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if known.contains(&name) {
                continue;
            }
            if let Some(meta) = read_project_json(&entry.path()).await? {
                let record = meta.to_record();
                if db.get_project_by_slug(&record.slug)?.is_none() {
                    db.insert_project(&record)?;
                }
                known.insert(record.slug.clone());
                projects.push(record);
            }
        }
    }
    projects.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(projects)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Platform App"), "platform-app");
        assert_eq!(slugify("  Hello!! "), "hello");
        assert_eq!(slugify(""), "project");
    }

    #[test]
    fn validate_slug_ok() {
        assert!(validate_slug("platform").is_ok());
        assert!(validate_slug("a_b-1").is_ok());
        assert!(validate_slug("Bad").is_err());
        assert!(validate_slug("-x").is_err());
    }

    #[tokio::test]
    async fn json_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("platform");
        let meta = ProjectJson {
            id: "abc".into(),
            slug: "platform".into(),
            name: "Platform".into(),
            default_branch: Some("main".into()),
            members: vec![ProjectMemberJson {
                owner: "alice".into(),
                name: "anchor".into(),
                full_name: "alice/anchor".into(),
                clone_url: "https://github.com/alice/anchor.git".into(),
                private: true,
                default_branch: "main".into(),
            }],
        };
        write_project_json(&dir, &meta).await.unwrap();
        let loaded = read_project_json(&dir).await.unwrap().unwrap();
        assert_eq!(loaded, meta);
    }

    #[tokio::test]
    async fn save_and_list() {
        let tmp = TempDir::new().unwrap();
        let db = Db::open(tmp.path().join("db.sqlite")).unwrap();
        let projects = tmp.path().join("projects");
        let record = ProjectRecord {
            id: new_project_id(),
            slug: "demo".into(),
            name: "Demo".into(),
            default_branch: None,
            created_at: Utc::now().to_rfc3339(),
            repos: vec![],
        };
        save_project(&db, &projects, &record).await.unwrap();
        let list = list_all_projects(&db, &projects).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].slug, "demo");
    }
}
