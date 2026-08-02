//! Anchor — GitHub → worktrees → tmux orchestrator.

mod api;
mod auth;
mod config;
mod dashboard;
mod db;
mod error;
mod git;
mod github;
mod project_store;
mod projects;
mod settings;
mod shell;
mod status_cache;
mod sync_memory;
mod terminal;
mod tmux;

use std::net::SocketAddr;
use std::sync::Arc;

use tracing_subscriber::EnvFilter;

use crate::auth::AuthConfig;
use crate::config::Config;
use crate::db::Db;
use crate::github::GitHubClient;
use crate::status_cache::StatusCache;
use crate::sync_memory::SyncMemory;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub github: Arc<GitHubClient>,
    pub sync_memory: SyncMemory,
    pub status_cache: StatusCache,
    pub db: Db,
    pub auth: Arc<AuthConfig>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env()?;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
        )
        .with_writer(crate::error::RedactingMakeWriter::stdout())
        .init();

    let db = Db::open(&config.database_path)?;
    tracing::info!(path = %db.path().display(), "opened settings database");

    // Migrate legacy 1:1 project dirs into sibling workspace layout.
    match git::migrate_all_legacy(&config.projects_dir, &config.github_user).await {
        Ok(migrated) if !migrated.is_empty() => {
            tracing::info!(?migrated, "migrated legacy project layouts");
            for slug in &migrated {
                // Seed DB + project.json when missing.
                if db.get_project_by_slug(slug)?.is_none() {
                    let owner = &config.github_user;
                    let full = format!("{owner}/{slug}");
                    let record = crate::db::ProjectRecord {
                        id: project_store::new_project_id(),
                        slug: slug.clone(),
                        name: slug.clone(),
                        default_branch: Some("main".into()),
                        created_at: chrono::Utc::now().to_rfc3339(),
                        repos: vec![crate::db::ProjectRepoRecord {
                            owner: owner.clone(),
                            name: slug.clone(),
                            full_name: full.clone(),
                            clone_url: format!(
                                "https://{}/{}.git",
                                config.github_host, full
                            ),
                            private: false,
                            default_branch: "main".into(),
                        }],
                    };
                    if let Err(e) =
                        project_store::save_project(&db, &config.projects_dir, &record).await
                    {
                        tracing::warn!(slug, error = %e, "failed to seed migrated project metadata");
                    }
                }
            }
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "legacy project migration failed"),
    }

    let auth = Arc::new(AuthConfig::from_config(&config));
    let github = Arc::new(GitHubClient::new(
        config.github_token.clone(),
        config.github_user.clone(),
        config.github_api_url.clone(),
    ));
    // Warm /user/repos cache so dashboard repos partial rarely cold-misses.
    {
        let gh = Arc::clone(&github);
        tokio::spawn(async move {
            if let Err(e) = gh.list_repos().await {
                tracing::warn!(error = %e, "background GitHub repo cache warm failed");
            }
        });
    }
    let state = AppState {
        config: Arc::new(config),
        github,
        sync_memory: SyncMemory::new(),
        status_cache: StatusCache::new(),
        db,
        auth,
    };

    let app = api::router(state.clone());
    let addr = SocketAddr::from(([0, 0, 0, 0], state.config.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
