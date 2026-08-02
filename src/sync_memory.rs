//! In-memory last-sync outcomes for operator UI (no DB).

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::RwLock;

use crate::error::redact_secrets;
use crate::git::WorktreeAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncOutcome {
    Ok,
    Failed,
    Partial,
}

#[derive(Debug, Clone, Serialize)]
pub struct LastSync {
    pub outcome: SyncOutcome,
    pub message: String,
    pub at: DateTime<Utc>,
    pub skipped_dirty: usize,
    pub skipped_diverged: usize,
}

impl LastSync {
    pub fn chip_class(&self) -> &'static str {
        match self.outcome {
            SyncOutcome::Ok => "ok",
            SyncOutcome::Partial => "warn",
            SyncOutcome::Failed => "dirty",
        }
    }

    pub fn label(&self) -> &'static str {
        match self.outcome {
            SyncOutcome::Ok => "ok",
            SyncOutcome::Partial => "partial",
            SyncOutcome::Failed => "failed",
        }
    }

    pub fn is_failed(&self) -> bool {
        matches!(self.outcome, SyncOutcome::Failed)
    }
}

#[derive(Clone, Default)]
pub struct SyncMemory {
    inner: Arc<RwLock<HashMap<String, LastSync>>>,
}

impl SyncMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get(&self, repo: &str) -> Option<LastSync> {
        self.inner.read().await.get(repo).cloned()
    }

    pub async fn record_ok(
        &self,
        repo: &str,
        actions: &[(String, WorktreeAction)],
    ) {
        let skipped_dirty = actions
            .iter()
            .filter(|(_, a)| *a == WorktreeAction::SkippedDirty)
            .count();
        let skipped_diverged = actions
            .iter()
            .filter(|(_, a)| *a == WorktreeAction::SkippedDiverged)
            .count();
        let outcome = if skipped_dirty + skipped_diverged > 0 {
            SyncOutcome::Partial
        } else {
            SyncOutcome::Ok
        };
        let message = if skipped_dirty + skipped_diverged == 0 {
            "synced".into()
        } else {
            format!(
                "synced with skips (dirty={skipped_dirty}, diverged={skipped_diverged})"
            )
        };
        self.inner.write().await.insert(
            repo.to_string(),
            LastSync {
                outcome,
                message,
                at: Utc::now(),
                skipped_dirty,
                skipped_diverged,
            },
        );
    }

    pub async fn record_err(&self, repo: &str, err: &str) {
        self.inner.write().await.insert(
            repo.to_string(),
            LastSync {
                outcome: SyncOutcome::Failed,
                message: redact_secrets(err),
                at: Utc::now(),
                skipped_dirty: 0,
                skipped_diverged: 0,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn records_failure_redacted() {
        let mem = SyncMemory::new();
        mem.record_err("demo", "Bearer ghp_LeakToken99 denied")
            .await;
        let last = mem.get("demo").await.unwrap();
        assert_eq!(last.outcome, SyncOutcome::Failed);
        assert!(!last.message.contains("LeakToken99"));
    }

    #[tokio::test]
    async fn records_partial_on_dirty_skip() {
        let mem = SyncMemory::new();
        mem.record_ok(
            "demo",
            &[
                ("cursor".into(), WorktreeAction::SkippedDirty),
                ("opencode".into(), WorktreeAction::AlreadyUpToDate),
            ],
        )
        .await;
        let last = mem.get("demo").await.unwrap();
        assert_eq!(last.outcome, SyncOutcome::Partial);
        assert_eq!(last.skipped_dirty, 1);
    }
}
