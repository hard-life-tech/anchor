//! HTTP routes per docs/api-contract.md.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;

use crate::dashboard;
use crate::error::AppError;
use crate::git::{self, WorktreeResult};
use crate::projects::{self, ProjectStatus};
use crate::tmux::{self, TmuxEnsureResult};
use crate::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/repos", get(list_repos))
        .route("/api/projects", get(list_projects))
        .route("/api/projects/{repo}/sync", post(sync_project))
        .route("/api/projects/{repo}/status", get(project_status))
        .merge(dashboard::routes())
        .with_state(state)
}

async fn healthz() -> &'static str {
    "OK"
}

async fn list_repos(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let list = state
        .github
        .list_repos()
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?;
    Ok(Json(list))
}

#[derive(Debug, Serialize)]
struct ProjectsResponse {
    projects: Vec<ProjectStatus>,
}

async fn list_projects(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let projects = projects::list_statuses(&state).await?;
    Ok(Json(ProjectsResponse { projects }))
}

async fn project_status(
    State(state): State<AppState>,
    Path(repo): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let bare = git::bare_dir(&state.config.projects_dir, &repo);
    if !bare.exists() {
        return Err(AppError::NotFound(format!("project not on disk: {repo}")));
    }
    Ok(Json(projects::build_status(&state, &repo).await?))
}

#[derive(Debug, Serialize)]
struct SyncResponse {
    name: String,
    created: bool,
    fetched: bool,
    worktrees: Vec<WorktreeResult>,
    tmux: TmuxEnsureResult,
}

async fn sync_project(
    State(state): State<AppState>,
    Path(repo): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let gh_repo = state
        .github
        .find_repo(&repo)
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("unknown GitHub repo: {repo}")))?;

    let sync_result = git::sync_project(
        &state.config.projects_dir,
        &repo,
        &gh_repo.clone_url,
        &gh_repo.default_branch,
        &state.config.github_token,
    )
    .await;

    let (created, fetched, worktrees) = match sync_result {
        Ok(v) => v,
        Err(e) => {
            let msg = e.to_string();
            state.sync_memory.record_err(&repo, &msg).await;
            return Err(AppError::BadGateway(msg));
        }
    };

    state.github.invalidate_cache().await;

    let actions: Vec<_> = worktrees
        .iter()
        .map(|w| (w.agent.clone(), w.action))
        .collect();
    state.sync_memory.record_ok(&repo, &actions).await;

    let cursor_cwd = git::worktree_dir(&state.config.projects_dir, &repo, "cursor");
    let opencode_cwd = git::worktree_dir(&state.config.projects_dir, &repo, "opencode");

    let tmux = match tmux::ensure_project_window(
        &state.config.tmux_session,
        &repo,
        &cursor_cwd.to_string_lossy(),
        &opencode_cwd.to_string_lossy(),
        &state.config.cursor_cmd,
        &state.config.opencode_cmd,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => {
            let msg = e.to_string();
            state.sync_memory.record_err(&repo, &msg).await;
            return Err(AppError::BadGateway(msg));
        }
    };

    Ok(Json(SyncResponse {
        name: repo,
        created,
        fetched,
        worktrees,
        tmux,
    }))
}
