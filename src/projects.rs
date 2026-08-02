//! Project status derived from SQLite membership + git + tmux.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures_util::future::join_all;
use serde::Serialize;

use crate::db::{ProjectRecord, ProjectRepoRecord};
use crate::error::AppError;
use crate::git;
use crate::project_store;
use crate::sync_memory::LastSync;
use crate::tmux;
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct MemberStatus {
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub private: bool,
    pub default_branch: String,
    pub worktrees: Vec<git::WorktreeStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectStatus {
    pub slug: String,
    pub name: String,
    pub member_count: usize,
    pub members: Vec<MemberStatus>,
    pub on_disk: bool,
    pub tmux_window_exists: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<LastSync>,
}

/// Deep status for detail pages / single-project API (git + tmux).
pub async fn build_status(state: &AppState, slug: &str) -> Result<ProjectStatus, AppError> {
    let record = project_store::ensure_db_has_project(
        &state.db,
        &state.config.projects_dir,
        slug,
    )
    .await
    .map_err(AppError::Other)?
    .ok_or_else(|| AppError::NotFound(format!("unknown project: {slug}")))?;

    let projects_dir = state.config.projects_dir.clone();
    let session = state.config.tmux_session.clone();
    let slug_owned = slug.to_string();

    let members_fut = async {
        let futs = record.repos.iter().map(|repo| {
            let projects_dir = projects_dir.clone();
            let slug = slug_owned.clone();
            let repo = repo.clone();
            async move {
                let worktrees = git::member_worktree_statuses(
                    &projects_dir,
                    &slug,
                    &repo.owner,
                    &repo.name,
                    &repo.default_branch,
                )
                .await
                .unwrap_or_default();
                MemberStatus {
                    owner: repo.owner,
                    name: repo.name,
                    full_name: repo.full_name,
                    private: repo.private,
                    default_branch: repo.default_branch,
                    worktrees,
                }
            }
        });
        join_all(futs).await
    };

    let tmux_fut = async {
        tmux::window_exists(&session, &slug_owned)
            .await
            .unwrap_or(false)
    };

    let last_sync_fut = state.sync_memory.get(slug);

    let (members, tmux_window_exists, last_sync) =
        tokio::join!(members_fut, tmux_fut, last_sync_fut);

    let last_synced = last_sync.as_ref().map(|s| s.at);
    let on_disk = state.config.projects_dir.join(slug).exists();

    Ok(ProjectStatus {
        slug: record.slug,
        name: record.name,
        member_count: members.len(),
        members,
        on_disk,
        tmux_window_exists,
        last_synced,
        last_sync,
    })
}

/// Cheap list inventory: DB + filesystem presence + one tmux list-windows.
/// Does **not** call GitHub or spawn per-row git status.
pub async fn list_statuses(state: &AppState) -> Result<Vec<ProjectStatus>, AppError> {
    if let Some(cached) = state.status_cache.get().await {
        return Ok(cached);
    }

    let projects = list_statuses_uncached(state).await?;
    state.status_cache.set(projects.clone()).await;
    Ok(projects)
}

async fn list_statuses_uncached(state: &AppState) -> Result<Vec<ProjectStatus>, AppError> {
    // Migration runs at process start — do not re-scan on every dashboard refresh.
    let records = project_store::list_all_projects(&state.db, &state.config.projects_dir)
        .await
        .map_err(AppError::Other)?;

    let mut seen: HashSet<String> = records.iter().map(|r| r.slug.clone()).collect();
    let mut all_records = records;

    // Legacy dirs without project.json (rare after startup migrate).
    let disk_slugs = git::list_on_disk_slugs(&state.config.projects_dir)
        .await
        .map_err(AppError::Other)?;
    for slug in disk_slugs {
        if seen.contains(&slug) {
            continue;
        }
        if let Ok(Some(rec)) =
            project_store::ensure_db_has_project(&state.db, &state.config.projects_dir, &slug).await
        {
            seen.insert(slug);
            all_records.push(rec);
        }
    }

    let window_names: HashSet<String> = tmux::list_window_names(&state.config.tmux_session)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect();

    let projects_dir = Arc::new(state.config.projects_dir.clone());
    let futs = all_records.into_iter().map(|record| {
        let projects_dir = Arc::clone(&projects_dir);
        let windows = window_names.clone();
        let sync_memory = state.sync_memory.clone();
        async move { build_list_status(&projects_dir, &sync_memory, record, &windows).await }
    });
    let mut projects = join_all(futs).await;
    projects.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(projects)
}

async fn build_list_status(
    projects_dir: &std::path::Path,
    sync_memory: &crate::sync_memory::SyncMemory,
    record: ProjectRecord,
    window_names: &HashSet<String>,
) -> ProjectStatus {
    let members: Vec<MemberStatus> = record
        .repos
        .iter()
        .map(|repo| member_from_record_cheap(projects_dir, &record.slug, repo))
        .collect();

    let last_sync = sync_memory.get(&record.slug).await;
    let last_synced = last_sync.as_ref().map(|s| s.at);
    let on_disk = projects_dir.join(&record.slug).exists();
    let tmux_window_exists = window_names.contains(&record.slug);

    ProjectStatus {
        slug: record.slug,
        name: record.name,
        member_count: members.len(),
        members,
        on_disk,
        tmux_window_exists,
        last_synced,
        last_sync,
    }
}

fn member_from_record_cheap(
    projects_dir: &std::path::Path,
    slug: &str,
    repo: &ProjectRepoRecord,
) -> MemberStatus {
    let worktrees =
        git::member_worktree_presence(projects_dir, slug, &repo.owner, &repo.name);
    MemberStatus {
        owner: repo.owner.clone(),
        name: repo.name.clone(),
        full_name: repo.full_name.clone(),
        // Visibility from stored membership — never hits GitHub on list.
        private: repo.private,
        default_branch: repo.default_branch.clone(),
        worktrees,
    }
}

pub fn repo_record_from_github(r: &crate::github::Repo) -> ProjectRepoRecord {
    let (owner, name) = split_full_name(&r.full_name).unwrap_or_else(|| {
        (
            String::new(),
            r.name.clone(),
        )
    });
    ProjectRepoRecord {
        owner: if owner.is_empty() {
            // Fallback — caller should prefer full_name.
            "unknown".into()
        } else {
            owner
        },
        name,
        full_name: r.full_name.clone(),
        clone_url: r.clone_url.clone(),
        private: r.private,
        default_branch: r.default_branch.clone(),
    }
}

pub fn split_full_name(full: &str) -> Option<(String, String)> {
    let (owner, name) = full.split_once('/')?;
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        return None;
    }
    Some((owner.to_string(), name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::sync_memory::SyncMemory;
    use tempfile::TempDir;

    #[tokio::test]
    async fn cheap_list_uses_db_private_not_git() {
        let tmp = TempDir::new().unwrap();
        let projects_dir = tmp.path().join("projects");
        tokio::fs::create_dir_all(&projects_dir).await.unwrap();
        let db_path = tmp.path().join("t.db");
        let db = Db::open(&db_path).unwrap();

        let record = ProjectRecord {
            id: "id1".into(),
            slug: "platform".into(),
            name: "Platform".into(),
            default_branch: None,
            created_at: Utc::now().to_rfc3339(),
            repos: vec![ProjectRepoRecord {
                owner: "alice".into(),
                name: "anchor".into(),
                full_name: "alice/anchor".into(),
                clone_url: "https://github.com/alice/anchor.git".into(),
                private: true,
                default_branch: "main".into(),
            }],
        };
        project_store::save_project(&db, &projects_dir, &record)
            .await
            .unwrap();

        let windows = HashSet::new();
        let sync = SyncMemory::new();
        let status = build_list_status(&projects_dir, &sync, record, &windows).await;
        assert_eq!(status.members.len(), 1);
        assert!(status.members[0].private);
        assert!(status.members[0].worktrees.is_empty()); // not synced yet
        assert!(status.on_disk);
        assert!(!status.tmux_window_exists);
    }
}
