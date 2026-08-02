//! On-disk project status derived from git + tmux (no DB).

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::error::AppError;
use crate::git;
use crate::sync_memory::LastSync;
use crate::tmux;
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct ProjectStatus {
    pub name: String,
    pub on_disk: bool,
    pub worktrees: Vec<git::WorktreeStatus>,
    pub tmux_window_exists: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<LastSync>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<&'static str>,
}

pub async fn build_status(state: &AppState, name: &str) -> Result<ProjectStatus, AppError> {
    let gh = state.github.find_repo(name).await.ok().flatten();
    let default_branch = gh
        .as_ref()
        .map(|r| r.default_branch.clone())
        .unwrap_or_else(|| "main".into());
    let visibility = gh.as_ref().map(|r| {
        if r.private {
            "private"
        } else {
            "public"
        }
    });

    let worktrees = git::project_worktree_statuses(&state.config.projects_dir, name, &default_branch)
        .await
        .map_err(AppError::Other)?;

    let tmux_window_exists = tmux::session_exists(&state.config.tmux_session)
        .await
        .unwrap_or(false)
        && tmux::window_exists(&state.config.tmux_session, name)
            .await
            .unwrap_or(false);

    let last_sync = state.sync_memory.get(name).await;
    let last_synced = last_sync.as_ref().map(|s| s.at);

    Ok(ProjectStatus {
        name: name.into(),
        on_disk: true,
        worktrees,
        tmux_window_exists,
        last_synced,
        last_sync,
        visibility,
    })
}

pub async fn list_statuses(state: &AppState) -> Result<Vec<ProjectStatus>, AppError> {
    let names = git::list_on_disk_projects(&state.config.projects_dir)
        .await
        .map_err(AppError::Other)?;
    let mut projects = Vec::new();
    for name in names {
        projects.push(build_status(state, &name).await?);
    }
    Ok(projects)
}
