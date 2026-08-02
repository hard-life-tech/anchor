//! Askama + htmx operator dashboard.

use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;

use crate::error::AppError;
use crate::git;
use crate::github::Repo;
use crate::projects::{self, ProjectStatus};
use crate::tmux;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(dashboard))
        .route("/static/style.css", get(style_css))
        .route("/partials/projects", get(partial_projects))
        .route("/partials/repos", get(partial_repos))
        .route("/partials/projects/{repo}/sync", post(partial_sync))
}

async fn style_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../static/style.css"),
    )
}

#[derive(Template, WebTemplate)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    healthy: bool,
    projects_dir: String,
    projects: Vec<ProjectStatus>,
    repos: Vec<RepoRow>,
    github_user: String,
}

#[derive(Template, WebTemplate)]
#[template(path = "partials/projects.html")]
struct ProjectsPartial {
    projects: Vec<ProjectStatus>,
}

#[derive(Template, WebTemplate)]
#[template(path = "partials/repos.html")]
struct ReposPartial {
    repos: Vec<RepoRow>,
    github_user: String,
}

#[derive(Debug, Clone)]
struct RepoRow {
    name: String,
    full_name: String,
    private: bool,
    default_branch: String,
    on_disk: bool,
}

async fn dashboard(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let projects = projects::list_statuses(&state).await.unwrap_or_default();
    let repos = load_repo_rows(&state).await.unwrap_or_default();
    Ok(DashboardTemplate {
        healthy: true,
        projects_dir: state.config.projects_dir.display().to_string(),
        projects,
        repos,
        github_user: state.config.github_user.clone(),
    })
}

async fn partial_projects(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let projects = projects::list_statuses(&state).await?;
    Ok(ProjectsPartial { projects })
}

async fn partial_repos(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let repos = load_repo_rows(&state)
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?;
    Ok(ReposPartial {
        repos,
        github_user: state.config.github_user.clone(),
    })
}

async fn partial_sync(
    State(state): State<AppState>,
    Path(repo): Path<String>,
) -> Result<Response, AppError> {
    let sync_result = run_sync(&state, &repo).await;
    let projects = projects::list_statuses(&state).await.unwrap_or_default();
    let projects_html = ProjectsPartial { projects }
        .render()
        .map_err(|e| AppError::Other(e.into()))?;

    match sync_result {
        Ok(()) => {
            let flash = format!(
                r#"<div class="flash" id="flash" hx-swap-oob="true">Synced <span class="mono">{}</span>.</div>"#,
                html_escape(&repo)
            );
            Ok(Html(format!("{flash}{projects_html}")).into_response())
        }
        Err(e) => {
            let msg = html_escape(&e.to_string());
            let flash = format!(
                r#"<div class="flash error" id="flash" hx-swap-oob="true">Sync failed: {msg}</div>"#
            );
            // 200 so htmx still swaps the projects partial + OOB flash.
            Ok(Html(format!("{flash}{projects_html}")).into_response())
        }
    }
}

async fn run_sync(state: &AppState, repo: &str) -> Result<(), AppError> {
    let gh_repo = state
        .github
        .find_repo(repo)
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("unknown GitHub repo: {repo}")))?;

    let (_created, _fetched, _worktrees) = git::sync_project(
        &state.config.projects_dir,
        repo,
        &gh_repo.clone_url,
        &gh_repo.default_branch,
        &state.config.github_token,
    )
    .await
    .map_err(|e| AppError::BadGateway(e.to_string()))?;

    let cursor_cwd = git::worktree_dir(&state.config.projects_dir, repo, "cursor");
    let opencode_cwd = git::worktree_dir(&state.config.projects_dir, repo, "opencode");

    tmux::ensure_project_window(
        &state.config.tmux_session,
        repo,
        &cursor_cwd.to_string_lossy(),
        &opencode_cwd.to_string_lossy(),
        &state.config.cursor_cmd,
        &state.config.opencode_cmd,
    )
    .await
    .map_err(|e| AppError::BadGateway(e.to_string()))?;

    Ok(())
}

async fn load_repo_rows(state: &AppState) -> anyhow::Result<Vec<RepoRow>> {
    let list = state.github.list_repos().await?;
    let on_disk = git::list_on_disk_projects(&state.config.projects_dir)
        .await
        .unwrap_or_default();
    Ok(list
        .repos
        .into_iter()
        .map(|r: Repo| RepoRow {
            name: r.name.clone(),
            full_name: r.full_name,
            private: r.private,
            default_branch: r.default_branch,
            on_disk: on_disk.iter().any(|n| n == &r.name),
        })
        .collect())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_partial_renders_empty() {
        let html = ProjectsPartial {
            projects: vec![],
        }
        .render()
        .unwrap();
        assert!(html.contains("No projects yet"));
    }

    #[test]
    fn projects_partial_renders_row() {
        let html = ProjectsPartial {
            projects: vec![ProjectStatus {
                name: "anchor".into(),
                on_disk: true,
                worktrees: vec![git::WorktreeStatus {
                    agent: "cursor".into(),
                    branch: "agent/cursor".into(),
                    ahead: 0,
                    behind: 1,
                    dirty: false,
                    diverged: false,
                }],
                tmux_window_exists: false,
                last_synced: None,
            }],
        }
        .render()
        .unwrap();
        assert!(html.contains("anchor"));
        assert!(html.contains("cursor"));
        assert!(html.contains("none"));
        assert!(html.contains("Sync"));
    }

    #[test]
    fn repos_partial_renders() {
        let html = ReposPartial {
            github_user: "alice".into(),
            repos: vec![RepoRow {
                name: "anchor".into(),
                full_name: "alice/anchor".into(),
                private: false,
                default_branch: "main".into(),
                on_disk: false,
            }],
        }
        .render()
        .unwrap();
        assert!(html.contains("alice/anchor"));
        assert!(html.contains(">Sync<"));
    }
}
