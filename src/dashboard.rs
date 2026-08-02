//! Askama + htmx operator dashboard.

use askama::Template;
use askama_web::WebTemplate;
use axum::extract::{Path, Query, RawForm, State};
use axum::http::header;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use chrono::Utc;
use serde::Deserialize;

use crate::api;
use crate::db::ProjectRecord;
use crate::error::AppError;
use crate::github::Repo;
use crate::project_store::{self, validate_slug};
use crate::projects::{self, ProjectStatus};
use crate::AppState;

/// First paint of the repos list — remaining rows load via "Show more".
const REPOS_PAGE_SIZE: usize = 40;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(dashboard))
        .route("/projects/{slug}", get(project_detail))
        .route("/partials/projects", get(partial_projects))
        .route("/partials/repos", get(partial_repos))
        .route("/partials/projects", post(partial_create_project))
        .route("/partials/projects/{slug}/sync", post(partial_sync))
        .route(
            "/partials/projects/{slug}/repos",
            post(partial_add_repo),
        )
}

/// Public (no auth) — needed by the login page.
pub async fn style_css() -> impl IntoResponse {
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
    github_user: String,
    github_host: String,
    create_repos: Vec<CreateRepoOption>,
}

#[derive(Debug, Clone)]
struct CreateRepoOption {
    full_name: String,
    private: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "project_detail.html")]
struct ProjectDetailTemplate {
    project: ProjectStatus,
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
    projects: Vec<ProjectOption>,
    github_user: String,
    github_host: String,
    offset: usize,
    has_more: bool,
    next_offset: usize,
}

#[derive(Debug, Clone)]
struct ProjectOption {
    slug: String,
    name: String,
}

#[derive(Debug, Clone)]
struct RepoRow {
    full_name: String,
    private: bool,
    default_branch: String,
    in_projects: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ReposQuery {
    #[serde(default)]
    offset: usize,
}

async fn dashboard(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    // Never block first paint on GitHub — use warm cache only; empty until warm.
    let create_repos = state
        .github
        .cached_repos()
        .await
        .map(|list| {
            list.repos
                .into_iter()
                .map(|r| CreateRepoOption {
                    full_name: r.full_name,
                    private: r.private,
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(DashboardTemplate {
        healthy: true,
        projects_dir: state.config.projects_dir.display().to_string(),
        github_user: state.config.github_user.clone(),
        github_host: state.config.github_host.clone(),
        create_repos,
    })
}

async fn project_detail(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let project = projects::build_status(&state, &slug).await?;
    Ok(ProjectDetailTemplate {
        project,
        github_user: state.config.github_user.clone(),
        github_host: state.config.github_host.clone(),
    })
}

async fn partial_projects(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let projects = projects::list_statuses(&state).await?;
    Ok(ProjectsPartial { projects })
}

async fn partial_repos(
    State(state): State<AppState>,
    Query(q): Query<ReposQuery>,
) -> Result<impl IntoResponse, AppError> {
    let (repos, has_more) = load_repo_rows_page(&state, q.offset, REPOS_PAGE_SIZE)
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?;
    let project_opts = state
        .db
        .list_projects()
        .map_err(AppError::Other)?
        .into_iter()
        .map(|p| ProjectOption {
            slug: p.slug,
            name: p.name,
        })
        .collect();
    let next_offset = q.offset.saturating_add(repos.len());
    Ok(ReposPartial {
        repos,
        projects: project_opts,
        github_user: state.config.github_user.clone(),
        github_host: state.config.github_host.clone(),
        offset: q.offset,
        has_more,
        next_offset,
    })
}

#[derive(Debug)]
struct CreateProjectForm {
    name: String,
    slug: String,
    repos: Vec<String>,
}

fn parse_create_form(bytes: &[u8]) -> CreateProjectForm {
    let mut name = String::new();
    let mut slug = String::new();
    let mut repos = Vec::new();
    for (k, v) in form_urlencoded::parse(bytes) {
        match k.as_ref() {
            "name" => name = v.into_owned(),
            "slug" => slug = v.into_owned(),
            "repos" => {
                let s = v.into_owned();
                if !s.is_empty() {
                    repos.push(s);
                }
            }
            _ => {}
        }
    }
    CreateProjectForm { name, slug, repos }
}

async fn partial_create_project(
    State(state): State<AppState>,
    RawForm(bytes): RawForm,
) -> Result<Response, AppError> {
    let form = parse_create_form(&bytes);
    let name = form.name.trim();
    if name.is_empty() {
        return Ok(flash_and_projects(
            &state,
            true,
            "Project name is required.",
        )
        .await);
    }
    let slug = {
        let s = form.slug.trim();
        if s.is_empty() {
            project_store::slugify(name)
        } else {
            s.to_string()
        }
    };
    if let Err(e) = validate_slug(&slug) {
        return Ok(flash_and_projects(&state, true, &e).await);
    }
    if state
        .db
        .get_project_by_slug(&slug)
        .map_err(AppError::Other)?
        .is_some()
    {
        return Ok(flash_and_projects(
            &state,
            true,
            &format!("Slug already exists: {slug}"),
        )
        .await);
    }

    let mut repos = Vec::new();
    for spec in &form.repos {
        match state.github.find_repo(spec).await {
            Ok(Some(gh)) => repos.push(projects::repo_record_from_github(&gh)),
            Ok(None) => {
                return Ok(flash_and_projects(
                    &state,
                    true,
                    &format!("Unknown GitHub repo: {spec}"),
                )
                .await);
            }
            Err(e) => {
                return Ok(flash_and_projects(&state, true, &e.to_string()).await);
            }
        }
    }

    let record = ProjectRecord {
        id: project_store::new_project_id(),
        slug: slug.clone(),
        name: name.to_string(),
        default_branch: None,
        created_at: Utc::now().to_rfc3339(),
        repos,
    };
    if let Err(e) =
        project_store::save_project(&state.db, &state.config.projects_dir, &record).await
    {
        return Ok(flash_and_projects(&state, true, &e.to_string()).await);
    }
    state.status_cache.invalidate().await;

    // Auto-sync after create when members present.
    if !record.repos.is_empty() {
        let _ = api::sync_all_members(&state, &record).await;
        state.status_cache.invalidate().await;
    }

    Ok(flash_and_projects(
        &state,
        false,
        &format!("Created project {slug}"),
    )
    .await)
}

#[derive(Debug)]
struct AddRepoForm {
    full_name: String,
    project_slug: String,
}

fn parse_add_repo_form(bytes: &[u8]) -> AddRepoForm {
    let mut full_name = String::new();
    let mut project_slug = String::new();
    for (k, v) in form_urlencoded::parse(bytes) {
        match k.as_ref() {
            "full_name" => full_name = v.into_owned(),
            "project_slug" => project_slug = v.into_owned(),
            _ => {}
        }
    }
    AddRepoForm {
        full_name,
        project_slug,
    }
}

async fn partial_add_repo(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    RawForm(bytes): RawForm,
) -> Result<Response, AppError> {
    let form = parse_add_repo_form(&bytes);
    let target = if form.project_slug.trim().is_empty() {
        slug
    } else {
        form.project_slug.trim().to_string()
    };
    let record = state
        .db
        .get_project_by_slug(&target)
        .map_err(AppError::Other)?
        .ok_or_else(|| AppError::NotFound(format!("unknown project: {target}")))?;

    let gh = state
        .github
        .find_repo(&form.full_name)
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("unknown GitHub repo: {}", form.full_name)))?;
    let repo = projects::repo_record_from_github(&gh);
    state
        .db
        .add_repo(&record.id, &repo)
        .map_err(AppError::Other)?;
    let record = state
        .db
        .get_project_by_slug(&target)
        .map_err(AppError::Other)?
        .unwrap();
    project_store::write_project_json(
        &state.config.projects_dir.join(&target),
        &project_store::ProjectJson::from(&record),
    )
    .await
    .map_err(AppError::Other)?;
    state.status_cache.invalidate().await;

    // Refresh repos partial (first page).
    let (repos, has_more) = load_repo_rows_page(&state, 0, REPOS_PAGE_SIZE)
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?;
    let project_opts = state
        .db
        .list_projects()
        .map_err(AppError::Other)?
        .into_iter()
        .map(|p| ProjectOption {
            slug: p.slug,
            name: p.name,
        })
        .collect();
    let next_offset = repos.len();
    let repos_html = ReposPartial {
        repos,
        projects: project_opts,
        github_user: state.config.github_user.clone(),
        github_host: state.config.github_host.clone(),
        offset: 0,
        has_more,
        next_offset,
    }
    .render()
    .map_err(|e| AppError::Other(e.into()))?;

    let flash = format!(
        r#"<div class="flash" id="flash" hx-swap-oob="true">Added <span class="mono">{}</span> to <span class="mono">{}</span>.</div>"#,
        html_escape(&form.full_name),
        html_escape(&target)
    );
    Ok(Html(format!("{flash}{repos_html}")).into_response())
}

async fn partial_sync(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Response, AppError> {
    let sync_result = run_sync(&state, &slug).await;
    state.status_cache.invalidate().await;
    let projects = projects::list_statuses(&state).await.unwrap_or_default();
    let projects_html = ProjectsPartial { projects }
        .render()
        .map_err(|e| AppError::Other(e.into()))?;

    match sync_result {
        Ok(summary) => {
            let flash = format!(
                r#"<div class="flash" id="flash" hx-swap-oob="true">Synced <span class="mono">{}</span> — {}.</div>"#,
                html_escape(&slug),
                html_escape(&summary)
            );
            Ok(Html(format!("{flash}{projects_html}")).into_response())
        }
        Err(e) => {
            let msg = html_escape(&e.safe_message());
            let flash = format!(
                r#"<div class="flash error" id="flash" hx-swap-oob="true">Sync failed: {msg}</div>"#
            );
            Ok(Html(format!("{flash}{projects_html}")).into_response())
        }
    }
}

async fn flash_and_projects(state: &AppState, is_error: bool, msg: &str) -> Response {
    let projects = projects::list_statuses(state).await.unwrap_or_default();
    let projects_html = ProjectsPartial { projects }
        .render()
        .unwrap_or_else(|_| "<p class=\"empty\">Error rendering projects</p>".into());
    let class = if is_error { "flash error" } else { "flash" };
    let flash = format!(
        r#"<div class="{class}" id="flash" hx-swap-oob="true">{}</div>"#,
        html_escape(msg)
    );
    Html(format!("{flash}{projects_html}")).into_response()
}

async fn run_sync(state: &AppState, slug: &str) -> Result<String, AppError> {
    // Ensure project exists (legacy single-repo wrap).
    let record = if let Some(r) = state
        .db
        .get_project_by_slug(slug)
        .map_err(AppError::Other)?
    {
        r
    } else {
        // Delegate to API ensure path via sync_all after creating from GitHub.
        return sync_via_api_ensure(state, slug).await;
    };

    api::sync_all_members(state, &record).await?;
    let last = state.sync_memory.get(slug).await;
    Ok(last
        .map(|s| s.message)
        .unwrap_or_else(|| "ok".into()))
}

async fn sync_via_api_ensure(state: &AppState, slug: &str) -> Result<String, AppError> {
    // Call the same legacy wrap as API: resolve GitHub + create + sync.
    let gh = state
        .github
        .find_repo(slug)
        .await
        .map_err(|e| AppError::BadGateway(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("unknown GitHub repo: {slug}")))?;
    let repo = projects::repo_record_from_github(&gh);
    let project_slug = if validate_slug(slug).is_ok() {
        slug.to_string()
    } else {
        project_store::slugify(&gh.name)
    };
    let record = ProjectRecord {
        id: project_store::new_project_id(),
        slug: project_slug.clone(),
        name: gh.name.clone(),
        default_branch: Some(gh.default_branch.clone()),
        created_at: Utc::now().to_rfc3339(),
        repos: vec![repo],
    };
    project_store::save_project(&state.db, &state.config.projects_dir, &record)
        .await
        .map_err(AppError::Other)?;
    api::sync_all_members(state, &record).await?;
    let last = state.sync_memory.get(&project_slug).await;
    Ok(last
        .map(|s| s.message)
        .unwrap_or_else(|| "ok".into()))
}

async fn load_repo_rows_page(
    state: &AppState,
    offset: usize,
    limit: usize,
) -> anyhow::Result<(Vec<RepoRow>, bool)> {
    let list = state.github.list_repos().await?;
    let projects = state.db.list_projects().unwrap_or_default();
    let total = list.repos.len();
    let end = offset.saturating_add(limit).min(total);
    let start = offset.min(total);
    let page = &list.repos[start..end];
    let rows: Vec<RepoRow> = page
        .iter()
        .map(|r: &Repo| {
            let in_projects: Vec<String> = projects
                .iter()
                .filter(|p| {
                    p.repos.iter().any(|m| {
                        m.full_name == r.full_name
                            || (m.name == r.name
                                && m.owner.as_str()
                                    == r.full_name.split('/').next().unwrap_or(""))
                    })
                })
                .map(|p| p.slug.clone())
                .collect();
            RepoRow {
                full_name: r.full_name.clone(),
                private: r.private,
                default_branch: r.default_branch.clone(),
                in_projects,
            }
        })
        .collect();
    let has_more = end < total;
    Ok((rows, has_more))
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
    use crate::git;
    use crate::projects::MemberStatus;
    use crate::sync_memory::{LastSync, SyncOutcome};
    use chrono::Utc;

    #[test]
    fn dashboard_shell_defers_lists() {
        let html = DashboardTemplate {
            healthy: true,
            projects_dir: "/home/agent/projects".into(),
            github_user: "alice".into(),
            github_host: "github.com".into(),
            create_repos: vec![],
        }
        .render()
        .unwrap();
        assert!(html.contains("hx-get=\"/partials/projects\""));
        assert!(html.contains("hx-get=\"/partials/repos\""));
        assert!(html.contains("hx-trigger=\"load\""));
        assert!(html.contains("skeleton-block"));
        assert!(html.contains("Create project") || html.contains("create-project"));
    }

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
                slug: "platform".into(),
                name: "Platform".into(),
                member_count: 1,
                members: vec![MemberStatus {
                    owner: "alice".into(),
                    name: "anchor".into(),
                    full_name: "alice/anchor".into(),
                    private: true,
                    default_branch: "main".into(),
                    worktrees: vec![git::WorktreeStatus {
                        agent: "cursor".into(),
                        branch: "agent/cursor".into(),
                        ahead: 0,
                        behind: 1,
                        dirty: false,
                        diverged: false,
                    }],
                }],
                on_disk: true,
                tmux_window_exists: false,
                last_synced: None,
                last_sync: Some(LastSync {
                    outcome: SyncOutcome::Failed,
                    message: "GitHub authentication required or denied".into(),
                    at: Utc::now(),
                    skipped_dirty: 0,
                    skipped_diverged: 0,
                }),
            }],
        }
        .render()
        .unwrap();
        assert!(html.contains("platform"));
        assert!(html.contains("1"));
        assert!(html.contains("Sync"));
    }

    #[test]
    fn repos_partial_renders() {
        let html = ReposPartial {
            github_user: "alice".into(),
            github_host: "github.com".into(),
            projects: vec![ProjectOption {
                slug: "platform".into(),
                name: "Platform".into(),
            }],
            repos: vec![RepoRow {
                full_name: "alice/anchor".into(),
                private: true,
                default_branch: "main".into(),
                in_projects: vec![],
            }],
            offset: 0,
            has_more: false,
            next_offset: 1,
        }
        .render()
        .unwrap();
        assert!(html.contains("alice/anchor"));
        assert!(html.contains("Add to project") || html.contains("add"));
        assert!(html.contains("private"));
        assert!(html.contains("github.com"));
    }

    #[test]
    fn repos_partial_show_more() {
        let html = ReposPartial {
            github_user: "alice".into(),
            github_host: "github.com".into(),
            projects: vec![],
            repos: vec![RepoRow {
                full_name: "alice/a".into(),
                private: false,
                default_branch: "main".into(),
                in_projects: vec![],
            }],
            offset: 0,
            has_more: true,
            next_offset: 40,
        }
        .render()
        .unwrap();
        assert!(html.contains("Show more"));
        assert!(html.contains("offset=40"));
    }

    #[test]
    fn dashboard_shell_does_not_block_on_create_repos() {
        let html = DashboardTemplate {
            healthy: true,
            projects_dir: "/tmp/p".into(),
            github_user: "alice".into(),
            github_host: "github.com".into(),
            create_repos: vec![],
        }
        .render()
        .unwrap();
        // Projects load immediately; repos staggered so projects paint first.
        assert!(html.contains("hx-get=\"/partials/projects\""));
        assert!(html.contains("hx-trigger=\"load delay:50ms\"") || html.contains("delay:50ms"));
    }
}
