//! HTTP routes per docs/api-contract.md.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::auth;
use crate::dashboard;
use crate::db::{ProjectRecord, ProjectRepoRecord};
use crate::error::AppError;
use crate::git::{self, WorktreeResult};
use crate::project_store::{self, validate_slug};
use crate::projects::{self, ProjectStatus};
use crate::settings;
use crate::terminal;
use crate::tmux::{self, TmuxEnsureResult};
use crate::AppState;

pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/api/repos", get(list_repos))
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/{slug}",
            get(get_project).patch(patch_project).delete(delete_project),
        )
        .route("/api/projects/{slug}/sync", post(sync_project))
        .route("/api/projects/{slug}/repos", post(add_repos))
        .route(
            "/api/projects/{slug}/repos/{owner}/{repo}",
            delete(remove_repo),
        )
        .route(
            "/api/projects/{slug}/repos/{owner}/{repo}/sync",
            post(sync_one_repo),
        )
        // Legacy alias kept for status path compatibility.
        .route("/api/projects/{slug}/status", get(get_project))
        .merge(dashboard::routes())
        .merge(settings::routes())
        .merge(terminal::routes())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    Router::new()
        .route("/healthz", get(healthz))
        .route("/static/style.css", get(dashboard::style_css))
        .merge(auth::routes())
        .merge(protected)
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

#[derive(Debug, Deserialize)]
struct CreateProjectBody {
    name: String,
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    repos: Vec<String>,
}

async fn create_project(
    State(state): State<AppState>,
    Json(body): Json<CreateProjectBody>,
) -> Result<impl IntoResponse, AppError> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    let slug = body
        .slug
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| project_store::slugify(name));
    if let Err(e) = validate_slug(&slug) {
        return Err(AppError::BadRequest(e));
    }
    if state
        .db
        .get_project_by_slug(&slug)
        .map_err(AppError::Other)?
        .is_some()
    {
        return Err(AppError::Conflict(format!("project slug already exists: {slug}")));
    }

    let mut repos = Vec::new();
    for spec in &body.repos {
        let gh = resolve_repo(&state, spec).await?;
        repos.push(projects::repo_record_from_github(&gh));
    }

    let record = ProjectRecord {
        id: project_store::new_project_id(),
        slug: slug.clone(),
        name: name.to_string(),
        default_branch: None,
        created_at: Utc::now().to_rfc3339(),
        repos,
    };
    project_store::save_project(&state.db, &state.config.projects_dir, &record)
        .await
        .map_err(AppError::Other)?;
    state.status_cache.invalidate().await;

    let status = projects::build_status(&state, &slug).await?;
    Ok((StatusCode::CREATED, Json(status)))
}

#[derive(Debug, Deserialize)]
struct PatchProjectBody {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    default_branch: Option<String>,
}

async fn get_project(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(projects::build_status(&state, &slug).await?))
}

async fn patch_project(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(body): Json<PatchProjectBody>,
) -> Result<impl IntoResponse, AppError> {
    let updated = state
        .db
        .update_project_meta(
            &slug,
            body.name.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            body.default_branch.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        )
        .map_err(AppError::Other)?;
    if !updated {
        return Err(AppError::NotFound(format!("unknown project: {slug}")));
    }
    // Refresh disk mirror.
    if let Some(record) = state.db.get_project_by_slug(&slug).map_err(AppError::Other)? {
        project_store::write_project_json(
            &state.config.projects_dir.join(&slug),
            &project_store::ProjectJson::from(&record),
        )
        .await
        .map_err(AppError::Other)?;
    }
    state.status_cache.invalidate().await;
    Ok(Json(projects::build_status(&state, &slug).await?))
}

async fn delete_project(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let deleted = state.db.delete_project(&slug).map_err(AppError::Other)?;
    if !deleted {
        return Err(AppError::NotFound(format!("unknown project: {slug}")));
    }
    let _ = project_store::delete_project_json(&state.config.projects_dir.join(&slug)).await;
    state.status_cache.invalidate().await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct AddReposBody {
    repos: Vec<String>,
}

async fn add_repos(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(body): Json<AddReposBody>,
) -> Result<impl IntoResponse, AppError> {
    let mut record = state
        .db
        .get_project_by_slug(&slug)
        .map_err(AppError::Other)?
        .ok_or_else(|| AppError::NotFound(format!("unknown project: {slug}")))?;

    for spec in &body.repos {
        let gh = resolve_repo(&state, spec).await?;
        let repo = projects::repo_record_from_github(&gh);
        state
            .db
            .add_repo(&record.id, &repo)
            .map_err(AppError::Other)?;
        if !record
            .repos
            .iter()
            .any(|r| r.owner == repo.owner && r.name == repo.name)
        {
            record.repos.push(repo);
        }
    }
    // Reload + mirror.
    let record = state
        .db
        .get_project_by_slug(&slug)
        .map_err(AppError::Other)?
        .unwrap();
    project_store::write_project_json(
        &state.config.projects_dir.join(&slug),
        &project_store::ProjectJson::from(&record),
    )
    .await
    .map_err(AppError::Other)?;

    state.status_cache.invalidate().await;
    Ok(Json(projects::build_status(&state, &slug).await?))
}

async fn remove_repo(
    State(state): State<AppState>,
    Path((slug, owner, repo)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let record = state
        .db
        .get_project_by_slug(&slug)
        .map_err(AppError::Other)?
        .ok_or_else(|| AppError::NotFound(format!("unknown project: {slug}")))?;

    let member = record
        .repos
        .iter()
        .find(|r| r.owner == owner && r.name == repo)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("member not in project: {owner}/{repo}")))?;

    if let Err(e) = git::remove_member_disk(
        &state.config.projects_dir,
        &slug,
        &owner,
        &repo,
        &member.default_branch,
    )
    .await
    {
        return Err(AppError::Conflict(e.to_string()));
    }

    state
        .db
        .remove_repo(&record.id, &owner, &repo)
        .map_err(AppError::Other)?;
    let record = state
        .db
        .get_project_by_slug(&slug)
        .map_err(AppError::Other)?
        .unwrap();
    project_store::write_project_json(
        &state.config.projects_dir.join(&slug),
        &project_store::ProjectJson::from(&record),
    )
    .await
    .map_err(AppError::Other)?;

    state.status_cache.invalidate().await;
    Ok(Json(projects::build_status(&state, &slug).await?))
}

#[derive(Debug, Serialize)]
pub(crate) struct MemberSyncResult {
    pub full_name: String,
    pub created: bool,
    pub fetched: bool,
    pub worktrees: Vec<WorktreeResult>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SyncResponse {
    pub slug: String,
    pub repos: Vec<MemberSyncResult>,
    pub tmux: TmuxEnsureResult,
}

async fn sync_project(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let record = ensure_project_for_sync(&state, &slug).await?;
    let result = sync_all_members(&state, &record).await?;
    Ok(Json(result))
}

async fn sync_one_repo(
    State(state): State<AppState>,
    Path((slug, owner, repo)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let record = state
        .db
        .get_project_by_slug(&slug)
        .map_err(AppError::Other)?
        .ok_or_else(|| AppError::NotFound(format!("unknown project: {slug}")))?;
    let member = record
        .repos
        .iter()
        .find(|r| r.owner == owner && r.name == repo)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("member not in project: {owner}/{repo}")))?;

    let one = sync_member_record(&state, &slug, &member).await?;
    let tmux = ensure_tmux(&state, &slug, &record.repos).await?;
    Ok(Json(serde_json::json!({
        "slug": slug,
        "repos": [one],
        "tmux": tmux,
    })))
}

/// Ensure a project exists for `{slug}` — create single-member from GitHub if needed (legacy wrap).
async fn ensure_project_for_sync(
    state: &AppState,
    slug: &str,
) -> Result<ProjectRecord, AppError> {
    let _ = git::migrate_legacy_project(
        &state.config.projects_dir,
        slug,
        &state.config.github_user,
    )
    .await;

    if let Some(mut record) =
        project_store::ensure_db_has_project(&state.db, &state.config.projects_dir, slug)
            .await
            .map_err(AppError::Other)?
    {
        // After legacy migrate, ensure JSON + DB have the member.
        if record.repos.is_empty() && git::is_legacy_layout(&state.config.projects_dir.join(slug))
        {
            // Should have been migrated; fall through.
        } else if record.repos.is_empty() {
            // Migrated disk without members in JSON — try to infer from .bares.
            if let Some(inferred) = infer_members_from_bares(state, slug).await? {
                for repo in &inferred {
                    state.db.add_repo(&record.id, repo).map_err(AppError::Other)?;
                }
                record = state
                    .db
                    .get_project_by_slug(slug)
                    .map_err(AppError::Other)?
                    .unwrap();
                project_store::write_project_json(
                    &state.config.projects_dir.join(slug),
                    &project_store::ProjectJson::from(&record),
                )
                .await
                .map_err(AppError::Other)?;
            }
        }
        if !record.repos.is_empty() {
            return Ok(record);
        }
    }

    // Legacy wrap: treat slug as GitHub short name / owner__name / create project.
    let gh = resolve_repo(state, slug).await?;
    let repo = projects::repo_record_from_github(&gh);
    let project_slug = if validate_slug(slug).is_ok() {
        slug.to_string()
    } else {
        project_store::slugify(&gh.name)
    };
    if let Err(e) = validate_slug(&project_slug) {
        return Err(AppError::BadRequest(e));
    }

    if let Some(existing) = state
        .db
        .get_project_by_slug(&project_slug)
        .map_err(AppError::Other)?
    {
        if !existing
            .repos
            .iter()
            .any(|r| r.owner == repo.owner && r.name == repo.name)
        {
            state
                .db
                .add_repo(&existing.id, &repo)
                .map_err(AppError::Other)?;
            let existing = state
                .db
                .get_project_by_slug(&project_slug)
                .map_err(AppError::Other)?
                .unwrap();
            project_store::write_project_json(
                &state.config.projects_dir.join(&project_slug),
                &project_store::ProjectJson::from(&existing),
            )
            .await
            .map_err(AppError::Other)?;
            return Ok(existing);
        }
        return Ok(existing);
    }

    let record = ProjectRecord {
        id: project_store::new_project_id(),
        slug: project_slug,
        name: gh.name.clone(),
        default_branch: Some(gh.default_branch.clone()),
        created_at: Utc::now().to_rfc3339(),
        repos: vec![repo],
    };
    project_store::save_project(&state.db, &state.config.projects_dir, &record)
        .await
        .map_err(AppError::Other)?;
    Ok(record)
}

async fn infer_members_from_bares(
    state: &AppState,
    slug: &str,
) -> Result<Option<Vec<ProjectRepoRecord>>, AppError> {
    let bares = state.config.projects_dir.join(slug).join(".bares");
    if !bares.is_dir() {
        return Ok(None);
    }
    let mut out = Vec::new();
    let mut entries = tokio::fs::read_dir(&bares)
        .await
        .map_err(|e| AppError::Other(e.into()))?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| AppError::Other(e.into()))?
    {
        if !entry
            .file_type()
            .await
            .map_err(|e| AppError::Other(e.into()))?
            .is_dir()
        {
            continue;
        }
        let key = entry.file_name().to_string_lossy().into_owned();
        let Some((owner, name)) = key.split_once("__") else {
            continue;
        };
        let full = format!("{owner}/{name}");
        if let Ok(Some(gh)) = state.github.find_repo(&full).await {
            out.push(projects::repo_record_from_github(&gh));
        } else {
            out.push(ProjectRepoRecord {
                owner: owner.into(),
                name: name.into(),
                full_name: full.clone(),
                clone_url: format!(
                    "https://{}/{}.git",
                    state.config.github_host, full
                ),
                private: false,
                default_branch: "main".into(),
            });
        }
    }
    if out.is_empty() {
        Ok(None)
    } else {
        Ok(Some(out))
    }
}

pub async fn sync_all_members(
    state: &AppState,
    record: &ProjectRecord,
) -> Result<SyncResponse, AppError> {
    let slug = &record.slug;
    let mut repos = Vec::new();
    let mut all_actions = Vec::new();

    for member in &record.repos {
        match sync_member_record(state, slug, member).await {
            Ok(r) => {
                for w in &r.worktrees {
                    all_actions.push((w.agent.clone(), w.action));
                }
                repos.push(r);
            }
            Err(e) => {
                let msg = e.safe_message();
                state.sync_memory.record_err(slug, &msg).await;
                return Err(e);
            }
        }
    }

    state.sync_memory.record_ok(slug, &all_actions).await;
    // Repo visibility list is unchanged by sync — keep the 180s GitHub cache.
    state.status_cache.invalidate().await;

    let tmux = ensure_tmux(state, slug, &record.repos).await?;
    Ok(SyncResponse {
        slug: slug.clone(),
        repos,
        tmux,
    })
}

async fn sync_member_record(
    state: &AppState,
    slug: &str,
    member: &ProjectRepoRecord,
) -> Result<MemberSyncResult, AppError> {
    let sync_result = git::sync_member(
        &state.config.projects_dir,
        slug,
        &member.owner,
        &member.name,
        &member.clone_url,
        &member.default_branch,
        &state.config.github_token,
    )
    .await;

    let (created, fetched, worktrees) = match sync_result {
        Ok(v) => v,
        Err(e) => {
            let msg = e.to_string();
            state.sync_memory.record_err(slug, &msg).await;
            return Err(AppError::BadGateway(msg));
        }
    };

    Ok(MemberSyncResult {
        full_name: member.full_name.clone(),
        created,
        fetched,
        worktrees,
    })
}

async fn ensure_tmux(
    state: &AppState,
    slug: &str,
    members: &[ProjectRepoRecord],
) -> Result<TmuxEnsureResult, AppError> {
    let cursor_cwd = git::agent_workspace(&state.config.projects_dir, slug, "cursor");
    let opencode_cwd = git::agent_workspace(&state.config.projects_dir, slug, "opencode");
    tokio::fs::create_dir_all(&cursor_cwd)
        .await
        .map_err(|e| AppError::Other(e.into()))?;
    tokio::fs::create_dir_all(&opencode_cwd)
        .await
        .map_err(|e| AppError::Other(e.into()))?;

    let (cursor_cmd, opencode_cmd) = settings::effective_cmds(state);
    let legacy: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();

    let result = if legacy.is_empty() {
        tmux::ensure_project_window(
            &state.config.tmux_session,
            slug,
            &cursor_cwd.to_string_lossy(),
            &opencode_cwd.to_string_lossy(),
            &cursor_cmd,
            &opencode_cmd,
        )
        .await
    } else {
        tmux::ensure_project_window_renaming(
            &state.config.tmux_session,
            slug,
            &cursor_cwd.to_string_lossy(),
            &opencode_cwd.to_string_lossy(),
            &cursor_cmd,
            &opencode_cmd,
            &legacy,
        )
        .await
    };
    result.map_err(|e| AppError::BadGateway(e.to_string()))
}

async fn resolve_repo(state: &AppState, spec: &str) -> Result<crate::github::Repo, AppError> {
    match state.github.find_repo(spec).await {
        Ok(Some(r)) => Ok(r),
        Ok(None) => Err(AppError::NotFound(format!("unknown GitHub repo: {spec}"))),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("ambiguous") {
                Err(AppError::BadRequest(msg))
            } else {
                Err(AppError::BadGateway(msg))
            }
        }
    }
}
