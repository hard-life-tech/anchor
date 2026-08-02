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
    github_host: String,
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
    github_host: String,
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
        github_host: state.config.github_host.clone(),
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
        github_host: state.config.github_host.clone(),
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
        Ok(summary) => {
            let flash = format!(
                r#"<div class="flash" id="flash" hx-swap-oob="true">Synced <span class="mono">{}</span> — {}.</div>"#,
                html_escape(&repo),
                html_escape(&summary)
            );
            Ok(Html(format!("{flash}{projects_html}")).into_response())
        }
        Err(e) => {
            let msg = html_escape(&e.safe_message());
            let flash = format!(
                r#"<div class="flash error" id="flash" hx-swap-oob="true">Sync failed: {msg}</div>"#
            );
            // 200 so htmx still swaps the projects partial + OOB flash.
            Ok(Html(format!("{flash}{projects_html}")).into_response())
        }
    }
}

async fn run_sync(state: &AppState, repo: &str) -> Result<String, AppError> {
    let gh_repo = state
        .github
        .find_repo(repo)
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("unknown GitHub repo: {repo}")))?;

    let sync_result = git::sync_project(
        &state.config.projects_dir,
        repo,
        &gh_repo.clone_url,
        &gh_repo.default_branch,
        &state.config.github_token,
    )
    .await;

    let (_created, _fetched, worktrees) = match sync_result {
        Ok(v) => v,
        Err(e) => {
            let msg = e.to_string();
            state.sync_memory.record_err(repo, &msg).await;
            return Err(AppError::BadGateway(msg));
        }
    };

    let actions: Vec<_> = worktrees
        .iter()
        .map(|w| (w.agent.clone(), w.action))
        .collect();
    state.sync_memory.record_ok(repo, &actions).await;

    let cursor_cwd = git::worktree_dir(&state.config.projects_dir, repo, "cursor");
    let opencode_cwd = git::worktree_dir(&state.config.projects_dir, repo, "opencode");

    if let Err(e) = tmux::ensure_project_window(
        &state.config.tmux_session,
        repo,
        &cursor_cwd.to_string_lossy(),
        &opencode_cwd.to_string_lossy(),
        &state.config.cursor_cmd,
        &state.config.opencode_cmd,
    )
    .await
    {
        let msg = e.to_string();
        state.sync_memory.record_err(repo, &msg).await;
        return Err(AppError::BadGateway(msg));
    }

    let last = state.sync_memory.get(repo).await;
    Ok(last
        .map(|s| s.message)
        .unwrap_or_else(|| "ok".into()))
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
    use crate::sync_memory::{LastSync, SyncOutcome};
    use chrono::Utc;

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
                last_sync: Some(LastSync {
                    outcome: SyncOutcome::Failed,
                    message: "GitHub authentication required or denied".into(),
                    at: Utc::now(),
                    skipped_dirty: 0,
                    skipped_diverged: 0,
                }),
                visibility: Some("private"),
            }],
        }
        .render()
        .unwrap();
        assert!(html.contains("anchor"));
        assert!(html.contains("cursor"));
        assert!(html.contains("none"));
        assert!(html.contains("Sync"));
        assert!(html.contains("private"));
        assert!(html.contains("failed") || html.contains("authentication"));
    }

    #[test]
    fn repos_partial_renders() {
        let html = ReposPartial {
            github_user: "alice".into(),
            github_host: "github.com".into(),
            repos: vec![RepoRow {
                name: "anchor".into(),
                full_name: "alice/anchor".into(),
                private: true,
                default_branch: "main".into(),
                on_disk: false,
            }],
        }
        .render()
        .unwrap();
        assert!(html.contains("alice/anchor"));
        assert!(html.contains(">Sync<"));
        assert!(html.contains("private"));
        assert!(html.contains("github.com"));
    }
}
