//! Project status derived from SQLite membership + git + tmux.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::db::ProjectRepoRecord;
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

pub async fn build_status(state: &AppState, slug: &str) -> Result<ProjectStatus, AppError> {
    let record = project_store::ensure_db_has_project(
        &state.db,
        &state.config.projects_dir,
        slug,
    )
    .await
    .map_err(AppError::Other)?
    .ok_or_else(|| AppError::NotFound(format!("unknown project: {slug}")))?;

    let mut members = Vec::new();
    for repo in &record.repos {
        let worktrees = git::member_worktree_statuses(
            &state.config.projects_dir,
            slug,
            &repo.owner,
            &repo.name,
            &repo.default_branch,
        )
        .await
        .map_err(AppError::Other)?;
        members.push(MemberStatus {
            owner: repo.owner.clone(),
            name: repo.name.clone(),
            full_name: repo.full_name.clone(),
            private: repo.private,
            default_branch: repo.default_branch.clone(),
            worktrees,
        });
    }

    let tmux_window_exists = tmux::session_exists(&state.config.tmux_session)
        .await
        .unwrap_or(false)
        && tmux::window_exists(&state.config.tmux_session, slug)
            .await
            .unwrap_or(false);

    let last_sync = state.sync_memory.get(slug).await;
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

pub async fn list_statuses(state: &AppState) -> Result<Vec<ProjectStatus>, AppError> {
    // Migrate any leftover legacy dirs before listing.
    let _ = git::migrate_all_legacy(&state.config.projects_dir, &state.config.github_user).await;

    let records = project_store::list_all_projects(&state.db, &state.config.projects_dir)
        .await
        .map_err(AppError::Other)?;

    // Also surface migrated legacy dirs that only have disk layout so far.
    let disk_slugs = git::list_on_disk_slugs(&state.config.projects_dir)
        .await
        .map_err(AppError::Other)?;

    let mut seen: std::collections::HashSet<String> =
        records.iter().map(|r| r.slug.clone()).collect();
    let mut projects = Vec::new();
    for record in records {
        projects.push(build_status(state, &record.slug).await?);
    }
    for slug in disk_slugs {
        if seen.contains(&slug) {
            continue;
        }
        // Legacy migrate may have left a dir without DB — synthesize single-member if possible.
        if let Ok(Some(_)) =
            project_store::ensure_db_has_project(&state.db, &state.config.projects_dir, &slug).await
        {
            projects.push(build_status(state, &slug).await?);
            seen.insert(slug);
        }
    }
    projects.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(projects)
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
