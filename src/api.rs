//! HTTP routes per docs/api-contract.md.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::error::AppError;
use crate::git::{self, WorktreeResult};
use crate::tmux::{self, TmuxEnsureResult};
use crate::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/repos", get(list_repos))
        .route("/api/projects", get(list_projects))
        .route("/api/projects/{repo}/sync", post(sync_project))
        .route("/api/projects/{repo}/status", get(project_status))
        .route("/", get(dashboard))
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

#[derive(Debug, Serialize)]
struct ProjectStatus {
    name: String,
    on_disk: bool,
    worktrees: Vec<git::WorktreeStatus>,
    tmux_window_exists: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_synced: Option<DateTime<Utc>>,
}

async fn list_projects(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let names = git::list_on_disk_projects(&state.config.projects_dir)
        .await
        .map_err(|e| AppError::Other(e))?;

    let mut projects = Vec::new();
    for name in names {
        projects.push(build_status(&state, &name).await?);
    }
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
    Ok(Json(build_status(&state, &repo).await?))
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

    let (created, fetched, worktrees) = git::sync_project(
        &state.config.projects_dir,
        &repo,
        &gh_repo.clone_url,
        &gh_repo.default_branch,
        &state.config.github_token,
    )
    .await
    .map_err(|e| AppError::BadGateway(e.to_string()))?;

    let cursor_cwd = git::worktree_dir(&state.config.projects_dir, &repo, "cursor");
    let opencode_cwd = git::worktree_dir(&state.config.projects_dir, &repo, "opencode");

    let tmux = tmux::ensure_project_window(
        &state.config.tmux_session,
        &repo,
        &cursor_cwd.to_string_lossy(),
        &opencode_cwd.to_string_lossy(),
        &state.config.cursor_cmd,
        &state.config.opencode_cmd,
    )
    .await
    .map_err(|e| AppError::BadGateway(e.to_string()))?;

    Ok(Json(SyncResponse {
        name: repo,
        created,
        fetched,
        worktrees,
        tmux,
    }))
}

async fn build_status(state: &AppState, name: &str) -> Result<ProjectStatus, AppError> {
    let default_branch = state
        .github
        .find_repo(name)
        .await
        .ok()
        .flatten()
        .map(|r| r.default_branch)
        .unwrap_or_else(|| "main".into());

    let worktrees = git::project_worktree_statuses(&state.config.projects_dir, name, &default_branch)
        .await
        .map_err(|e| AppError::Other(e))?;

    let tmux_window_exists = tmux::session_exists(&state.config.tmux_session)
        .await
        .unwrap_or(false)
        && tmux::window_exists(&state.config.tmux_session, name)
            .await
            .unwrap_or(false);

    Ok(ProjectStatus {
        name: name.into(),
        on_disk: true,
        worktrees,
        tmux_window_exists,
        last_synced: None,
    })
}

async fn dashboard(State(state): State<AppState>) -> impl IntoResponse {
    let names = git::list_on_disk_projects(&state.config.projects_dir)
        .await
        .unwrap_or_default();
    let body = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>Anchor</title>
<style>
body{{font-family:ui-sans-serif,system-ui,sans-serif;margin:2rem;background:#0f1419;color:#e7ecf1}}
h1{{font-weight:600;letter-spacing:-0.02em}}
a{{color:#7cb7ff}}
li{{margin:0.35rem 0}}
.meta{{color:#8b98a5;font-size:0.9rem}}
</style></head>
<body>
<h1>Anchor</h1>
<p class="meta">projects under {}</p>
<ul>
{}
</ul>
<p class="meta"><a href="/api/projects">/api/projects</a> · <a href="/api/repos">/api/repos</a> · <a href="/healthz">/healthz</a></p>
</body></html>"#,
        state.config.projects_dir.display(),
        if names.is_empty() {
            "<li class=\"meta\">no projects yet — POST /api/projects/{{repo}}/sync</li>".into()
        } else {
            names
                .iter()
                .map(|n| format!("<li><a href=\"/api/projects/{n}/status\">{n}</a></li>"))
                .collect::<Vec<_>>()
                .join("\n")
        }
    );
    (
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
}
