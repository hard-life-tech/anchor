//! GitHub REST client with short in-memory cache. Never logs the token.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

const CACHE_TTL: Duration = Duration::from_secs(180);
const PER_PAGE: u32 = 100;
const MAX_PAGES: u32 = 50;

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
    api_base: String,
    http: reqwest::Client,
    cache: Arc<RwLock<Option<CacheEntry>>>,
}

impl std::fmt::Debug for GitHubClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitHubClient")
            .field("token", &"[redacted]")
            .field("user", &self.user)
            .field("api_base", &self.api_base)
            .finish_non_exhaustive()
    }
}

impl GitHubClient {
    pub fn new(token: String, user: String, api_base: String) -> Self {
        let http = reqwest::Client::builder()
            .user_agent("hard-life-tech-anchor/0.1")
            .build()
            .expect("reqwest client");
        Self {
            token,
            user,
            api_base: api_base.trim_end_matches('/').to_string(),
            http,
            cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Build paginated `/user/repos` URL (authenticated; includes private + org).
    pub fn user_repos_page_url(&self, page: u32) -> String {
        format!(
            "{}/user/repos?per_page={PER_PAGE}&page={page}\
             &affiliation=owner,collaborator,organization_member&sort=full_name",
            self.api_base
        )
    }

    pub async fn list_repos(&self) -> Result<RepoList> {
        if let Some(list) = self.cached_repos().await {
            return Ok(list);
        }

        let mut repos = Vec::new();
        let mut page = 1u32;
        loop {
            let url = self.user_repos_page_url(page);
            let raw = self.fetch_repos_page(&url).await?;
            let count = raw.len() as u32;
            repos.extend(raw.into_iter().map(|r| Repo {
                name: r.name,
                full_name: r.full_name,
                private: r.private,
                default_branch: r.default_branch,
                clone_url: r.clone_url,
            }));
            if count < PER_PAGE || page >= MAX_PAGES {
                break;
            }
            page += 1;
        }

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

    /// Return a fresh cache hit, if any (within [`CACHE_TTL`]).
    pub async fn cached_repos(&self) -> Option<RepoList> {
        let guard = self.cache.read().await;
        guard.as_ref().and_then(|entry| {
            if entry.fetched_at.elapsed() < CACHE_TTL {
                Some(entry.list.clone())
            } else {
                None
            }
        })
    }

    /// Drop the in-memory `/user/repos` cache (e.g. after Sync changes on-disk state).
    pub async fn invalidate_cache(&self) {
        let mut guard = self.cache.write().await;
        *guard = None;
    }

    async fn fetch_repos_page(&self, url: &str) -> Result<Vec<GhRepo>> {
        let resp = self
            .http
            .get(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .context("github list repos")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let hint = match status.as_u16() {
                401 => " — invalid or expired GITHUB_TOKEN",
                403 => " — token lacks access or rate limited",
                404 => " — check GITHUB_API_URL for enterprise",
                _ => "",
            };
            return Err(anyhow!("GitHub API error: {status}{hint}"));
        }

        resp.json().await.context("decode repos")
    }

    pub async fn find_repo(&self, name: &str) -> Result<Option<Repo>> {
        let list = self.list_repos().await?;
        Ok(list.repos.into_iter().find(|r| r.name == name))
    }

    pub fn api_base(&self) -> &str {
        &self.api_base
    }

    pub fn user(&self) -> &str {
        &self.user
    }
}

#[derive(Deserialize)]
struct GhRepo {
    name: String,
    full_name: String,
    private: bool,
    default_branch: String,
    clone_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_repos_url_targets_authenticated_endpoint() {
        let gh = GitHubClient::new(
            "secret-token".into(),
            "alice".into(),
            "https://api.github.com".into(),
        );
        let url = gh.user_repos_page_url(1);
        assert!(url.starts_with("https://api.github.com/user/repos?"));
        assert!(url.contains("affiliation=owner,collaborator,organization_member"));
        // Must NOT use /users/{user}/repos (public-only).
        assert!(!url.contains("/users/"));
        assert!(!url.contains("secret-token"));
    }

    #[test]
    fn ghes_api_base_in_list_url() {
        let gh = GitHubClient::new(
            "t".into(),
            "alice".into(),
            "https://github.example.com/api/v3/".into(),
        );
        assert_eq!(gh.api_base(), "https://github.example.com/api/v3");
        let url = gh.user_repos_page_url(2);
        assert!(url.starts_with("https://github.example.com/api/v3/user/repos?"));
        assert!(url.contains("page=2"));
    }

    #[test]
    fn client_debug_redacts_token() {
        let gh = GitHubClient::new("super-secret".into(), "u".into(), "https://api.github.com".into());
        let dbg = format!("{gh:?}");
        assert!(!dbg.contains("super-secret"));
        assert!(dbg.contains("[redacted]"));
    }

    #[tokio::test]
    async fn cache_hit_and_invalidate() {
        let gh = GitHubClient::new("t".into(), "alice".into(), "https://api.github.com".into());
        assert!(gh.cached_repos().await.is_none());

        let list = RepoList {
            repos: vec![Repo {
                name: "anchor".into(),
                full_name: "alice/anchor".into(),
                private: true,
                default_branch: "main".into(),
                clone_url: "https://github.com/alice/anchor.git".into(),
            }],
            cached_at: Utc::now(),
        };
        {
            let mut guard = gh.cache.write().await;
            *guard = Some(CacheEntry {
                list: list.clone(),
                fetched_at: Instant::now(),
            });
        }

        let hit = gh.cached_repos().await.expect("fresh cache");
        assert_eq!(hit.repos.len(), 1);
        assert_eq!(hit.repos[0].name, "anchor");

        // list_repos must serve cache without network when fresh.
        let served = gh.list_repos().await.expect("from cache");
        assert_eq!(served.repos[0].full_name, "alice/anchor");

        gh.invalidate_cache().await;
        assert!(gh.cached_repos().await.is_none());
    }

    #[tokio::test]
    async fn expired_cache_is_miss() {
        let gh = GitHubClient::new("t".into(), "u".into(), "https://api.github.com".into());
        {
            let mut guard = gh.cache.write().await;
            *guard = Some(CacheEntry {
                list: RepoList {
                    repos: vec![],
                    cached_at: Utc::now(),
                },
                // Older than CACHE_TTL (180s).
                fetched_at: Instant::now() - CACHE_TTL - Duration::from_secs(1),
            });
        }
        assert!(gh.cached_repos().await.is_none());
    }
}
