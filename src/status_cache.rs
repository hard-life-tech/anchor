//! Short-TTL cache for dashboard/API project list status.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use crate::projects::ProjectStatus;

/// Aggressive enough for operator refresh, soft enough to absorb repeated hx-get.
const DEFAULT_TTL: Duration = Duration::from_secs(15);

#[derive(Clone, Default)]
pub struct StatusCache {
    inner: Arc<RwLock<Option<CacheEntry>>>,
    ttl: Duration,
}

struct CacheEntry {
    projects: Vec<ProjectStatus>,
    fetched_at: Instant,
}

impl StatusCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
            ttl: DEFAULT_TTL,
        }
    }

    #[cfg(test)]
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
            ttl,
        }
    }

    pub async fn get(&self) -> Option<Vec<ProjectStatus>> {
        let guard = self.inner.read().await;
        guard.as_ref().and_then(|entry| {
            if entry.fetched_at.elapsed() < self.ttl {
                Some(entry.projects.clone())
            } else {
                None
            }
        })
    }

    pub async fn set(&self, projects: Vec<ProjectStatus>) {
        let mut guard = self.inner.write().await;
        *guard = Some(CacheEntry {
            projects,
            fetched_at: Instant::now(),
        });
    }

    pub async fn invalidate(&self) {
        let mut guard = self.inner.write().await;
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::ProjectStatus;

    fn sample(slug: &str) -> ProjectStatus {
        ProjectStatus {
            slug: slug.into(),
            name: slug.into(),
            member_count: 0,
            members: vec![],
            on_disk: false,
            tmux_window_exists: false,
            last_synced: None,
            last_sync: None,
        }
    }

    #[tokio::test]
    async fn hit_miss_and_invalidate() {
        let cache = StatusCache::with_ttl(Duration::from_secs(60));
        assert!(cache.get().await.is_none());

        cache.set(vec![sample("a")]).await;
        let hit = cache.get().await.expect("fresh");
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].slug, "a");

        cache.invalidate().await;
        assert!(cache.get().await.is_none());
    }

    #[tokio::test]
    async fn expired_is_miss() {
        let cache = StatusCache::with_ttl(Duration::from_millis(20));
        cache.set(vec![sample("x")]).await;
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert!(cache.get().await.is_none());
    }
}
