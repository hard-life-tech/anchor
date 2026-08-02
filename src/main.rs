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
use crate::sync_memory::SyncMemory;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub github: Arc<GitHubClient>,
    pub sync_memory: SyncMemory,
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

    let auth = Arc::new(AuthConfig::from_config(&config));
    let github = GitHubClient::new(
        config.github_token.clone(),
        config.github_user.clone(),
        config.github_api_url.clone(),
    );
    let state = AppState {
        config: Arc::new(config),
        github: Arc::new(github),
        sync_memory: SyncMemory::new(),
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
