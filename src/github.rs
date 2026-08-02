//! GitHub REST client with short in-memory cache. Never logs the token.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

const CACHE_TTL: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub name: String,
    pub full_name: String,
    pub private: bool,
    pub default_branch: String,
    pub clone_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoList {
    pub repos: Vec<Repo>,
    pub cached_at: DateTime<Utc>,
}

#[derive(Clone)]
struct CacheEntry {
    list: RepoList,
    fetched_at: Instant,
}

pub struct GitHubClient {
    token: String,
    user: String,
    http: reqwest::Client,
    cache: Arc<RwLock<Option<CacheEntry>>>,
}

impl GitHubClient {
    pub fn new(token: String, user: String) -> Self {
        let http = reqwest::Client::builder()
            .user_agent("hard-life-tech-anchor/0.1")
            .build()
            .expect("reqwest client");
        Self {
            token,
            user,
            http,
            cache: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn list_repos(&self) -> Result<RepoList> {
        {
            let guard = self.cache.read().await;
            if let Some(entry) = guard.as_ref() {
                if entry.fetched_at.elapsed() < CACHE_TTL {
                    return Ok(entry.list.clone());
                }
            }
        }

        let url = format!("https://api.github.com/users/{}/repos?per_page=100&type=all", self.user);
        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .context("github list repos")?;

        if !resp.status().is_success() {
            let status = resp.status();
            // Do not include response body if it might echo auth issues with token fragments.
            return Err(anyhow!("GitHub API error: {status}"));
        }

        #[derive(Deserialize)]
        struct GhRepo {
            name: String,
            full_name: String,
            private: bool,
            default_branch: String,
            clone_url: String,
        }

        let raw: Vec<GhRepo> = resp.json().await.context("decode repos")?;
        let repos: Vec<Repo> = raw
            .into_iter()
            .map(|r| Repo {
                name: r.name,
                full_name: r.full_name,
                private: r.private,
                default_branch: r.default_branch,
                clone_url: r.clone_url,
            })
            .collect();

        let list = RepoList {
            repos,
            cached_at: Utc::now(),
        };
        {
            let mut guard = self.cache.write().await;
            *guard = Some(CacheEntry {
                list: list.clone(),
                fetched_at: Instant::now(),
            });
        }
        Ok(list)
    }

    pub async fn find_repo(&self, name: &str) -> Result<Option<Repo>> {
        let list = self.list_repos().await?;
        Ok(list.repos.into_iter().find(|r| r.name == name))
    }
}
