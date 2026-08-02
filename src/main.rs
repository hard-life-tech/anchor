//! Anchor — GitHub → worktrees → tmux orchestrator.

mod api;
mod config;
mod dashboard;
mod error;
mod git;
mod github;
mod projects;
mod shell;
mod tmux;

use std::net::SocketAddr;
use std::sync::Arc;

use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::github::GitHubClient;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub github: Arc<GitHubClient>,
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

    let github = GitHubClient::new(config.github_token.clone(), config.github_user.clone());
    let state = AppState {
        config: Arc::new(config),
        github: Arc::new(github),
    };

    let app = api::router(state.clone());
    let addr = SocketAddr::from(([0, 0, 0, 0], state.config.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
