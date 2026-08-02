//! Bare clone + worktree sync via shell-outs to `git`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use serde::Serialize;

use crate::error::{redact_secrets, redact_secrets_with_known};
use crate::shell::{self, CmdOutput};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeAction {
    Created,
    FastForwarded,
    AlreadyUpToDate,
    SkippedDirty,
    SkippedDiverged,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeResult {
    pub agent: String,
    pub action: WorktreeAction,
    pub dirty: bool,
    pub diverged: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeStatus {
    pub agent: String,
    pub branch: String,
    pub ahead: u32,
    pub behind: u32,
    pub dirty: bool,
    pub diverged: bool,
}

const AGENTS: &[(&str, &str)] = &[("cursor", "agent/cursor"), ("opencode", "agent/opencode")];

/// Process-local git HTTPS auth. Token stays in env values only — never in clone URL or argv.
#[derive(Clone)]
pub struct GitHttpsAuth {
    token: String,
}

impl std::fmt::Debug for GitHttpsAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitHttpsAuth")
            .field("token", &"[redacted]")
            .finish()
    }
}

impl std::fmt::Display for GitHttpsAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GitHttpsAuth([redacted])")
    }
}

impl GitHttpsAuth {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }

    /// `Authorization: Basic …` for GitHub HTTPS git (x-access-token).
    pub fn basic_authorization_header(&self) -> String {
        let credentials = format!("x-access-token:{}", self.token);
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes());
        format!("Authorization: Basic {encoded}")
    }

    /// Env for child `git`: header via `GIT_CONFIG_*` (not argv), no TTY prompt, no helper.
    pub fn env(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();
        // http.extraHeader + disable inherited credential.helper (avoids TTY username prompt).
        env.insert("GIT_CONFIG_COUNT".into(), "2".into());
        env.insert("GIT_CONFIG_KEY_0".into(), "http.extraHeader".into());
        env.insert(
            "GIT_CONFIG_VALUE_0".into(),
            self.basic_authorization_header(),
        );
        env.insert("GIT_CONFIG_KEY_1".into(), "credential.helper".into());
        env.insert("GIT_CONFIG_VALUE_1".into(), String::new());
        env.insert("GIT_TERMINAL_PROMPT".into(), "0".into());
        env
    }

    fn redact(&self, s: &str) -> String {
        let mut out = redact_secrets_with_known(s, &self.token);
        let header = self.basic_authorization_header();
        if let Some(b64) = header.strip_prefix("Authorization: Basic ") {
            out = out.replace(b64, "[redacted]");
        }
        out = out.replace(&header, "Authorization: Basic [redacted]");
        out
    }
}

fn is_auth_failure(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("could not read username")
        || lower.contains("authentication failed")
        || lower.contains("invalid username or password")
        || lower.contains("support for password authentication was removed")
        || lower.contains("http basic: access denied")
        || lower.contains("403")
        || lower.contains("401 unauthorized")
        || lower.contains("repository not found")
}

fn git_err(label: &str, out: &CmdOutput, auth: &GitHttpsAuth) -> anyhow::Error {
    let stderr = auth.redact(out.stderr.trim());
    if is_auth_failure(&stderr) {
        anyhow!(
            "{label} failed: GitHub authentication required or denied — \
             check GITHUB_TOKEN scope/access for this private or enterprise repo"
        )
    } else {
        anyhow!(
            "{label} failed (exit {}): {}",
            out.status,
            redact_secrets(&stderr)
        )
    }
}

fn ensure_git(label: &str, out: &CmdOutput, auth: &GitHttpsAuth) -> Result<()> {
    if out.success() {
        Ok(())
    } else {
        Err(git_err(label, out, auth))
    }
}

async fn ensure_origin_fetch_config(bare: &Path) -> Result<()> {
    let out = shell::run_git(
        &[
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ],
        Some(bare),
        &HashMap::new(),
    )
    .await?;
    out.ensure_success("git config remote.origin.fetch")?;
    Ok(())
}

async fn ensure_origin_ref(bare: &Path, default_branch: &str, origin_ref: &str) -> Result<()> {
    let has_origin = shell::run_git(
        &["rev-parse", "--verify", origin_ref],
        Some(bare),
        &HashMap::new(),
    )
    .await?
    .success();
    if has_origin {
        return Ok(());
    }
    // Fallback after local bare clone: mirror heads/<default> into remotes/origin/.
    let local = format!("refs/heads/{default_branch}");
    let remote = format!("refs/remotes/origin/{default_branch}");
    let out = shell::run_git(
        &["update-ref", &remote, &local],
        Some(bare),
        &HashMap::new(),
    )
    .await?;
    out.ensure_success("git update-ref origin ref")?;
    Ok(())
}

pub fn project_dir(projects_dir: &Path, repo: &str) -> PathBuf {
    projects_dir.join(repo)
}

pub fn bare_dir(projects_dir: &Path, repo: &str) -> PathBuf {
    project_dir(projects_dir, repo).join(".bare")
}

pub fn worktree_dir(projects_dir: &Path, repo: &str, agent: &str) -> PathBuf {
    project_dir(projects_dir, repo).join(agent)
}

/// Sync a project: create or update bare + worktrees. Idempotent; never force-overwrite dirty trees.
///
/// `clone_url` must be a normal HTTPS/SSH URL **without** embedded credentials.
pub async fn sync_project(
    projects_dir: &Path,
    repo: &str,
    clone_url: &str,
    default_branch: &str,
    token: &str,
) -> Result<(bool, bool, Vec<WorktreeResult>)> {
    let bare = bare_dir(projects_dir, repo);
    let auth = GitHttpsAuth::new(token);
    let env = auth.env();
    let mut created = false;

    if !bare.exists() {
        tokio::fs::create_dir_all(project_dir(projects_dir, repo))
            .await
            .context("create project dir")?;
        let out = shell::run_git(
            &["clone", "--bare", clone_url, &bare.to_string_lossy()],
            None,
            &env,
        )
        .await?;
        ensure_git("git clone --bare", &out, &auth)?;
        created = true;
    }

    // Bare clones default to mirroring into refs/heads/*. Point fetch at
    // refs/remotes/origin/* so worktrees can track origin/<default>.
    ensure_origin_fetch_config(&bare).await?;
    {
        let out = shell::run_git(&["fetch", "origin"], Some(&bare), &env).await?;
        ensure_git("git fetch", &out, &auth)?;
    }
    let fetched = true;

    // Ensure origin/<default> is visible for branch creation.
    let origin_ref = format!("origin/{default_branch}");
    ensure_origin_ref(&bare, default_branch, &origin_ref).await?;
    let mut results = Vec::new();

    for (agent, branch) in AGENTS {
        let wt = worktree_dir(projects_dir, repo, agent);
        if !wt.exists() {
            // Create branch from origin/default if needed, then add worktree.
            let _ = shell::run_git(
                &["rev-parse", "--verify", branch],
                Some(&bare),
                &HashMap::new(),
            )
            .await?;
            let exists = shell::run_git(
                &["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")],
                Some(&bare),
                &HashMap::new(),
            )
            .await?
            .success();

            if !exists {
                let out = shell::run_git(
                    &["branch", branch, &origin_ref],
                    Some(&bare),
                    &HashMap::new(),
                )
                .await?;
                out.ensure_success(&format!("git branch {branch}"))?;
            }

            let out = shell::run_git(
                &[
                    "worktree",
                    "add",
                    &wt.to_string_lossy(),
                    branch,
                ],
                Some(&bare),
                &HashMap::new(),
            )
            .await?;
            out.ensure_success(&format!("git worktree add {agent}"))?;
            results.push(WorktreeResult {
                agent: (*agent).into(),
                action: WorktreeAction::Created,
                dirty: false,
                diverged: false,
            });
            continue;
        }

        let status = worktree_status_in(&wt, &origin_ref).await?;
        if status.dirty {
            results.push(WorktreeResult {
                agent: (*agent).into(),
                action: WorktreeAction::SkippedDirty,
                dirty: true,
                diverged: status.diverged,
            });
            continue;
        }
        if status.diverged {
            results.push(WorktreeResult {
                agent: (*agent).into(),
                action: WorktreeAction::SkippedDiverged,
                dirty: false,
                diverged: true,
            });
            continue;
        }

        let before = rev_parse(&wt, "HEAD").await?;
        let out = shell::run_git(&["merge", "--ff-only", &origin_ref], Some(&wt), &HashMap::new())
            .await?;
        if !out.success() {
            // Treat ff failure as diverged skip (should be rare after checks).
            results.push(WorktreeResult {
                agent: (*agent).into(),
                action: WorktreeAction::SkippedDiverged,
                dirty: false,
                diverged: true,
            });
            continue;
        }
        let after = rev_parse(&wt, "HEAD").await?;
        let action = if before == after {
            WorktreeAction::AlreadyUpToDate
        } else {
            WorktreeAction::FastForwarded
        };
        results.push(WorktreeResult {
            agent: (*agent).into(),
            action,
            dirty: false,
            diverged: false,
        });
    }

    Ok((created, fetched, results))
}

pub async fn list_on_disk_projects(projects_dir: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    if !projects_dir.exists() {
        return Ok(names);
    }
    let mut entries = tokio::fs::read_dir(projects_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if bare_dir(projects_dir, &name).exists() {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

pub async fn project_worktree_statuses(
    projects_dir: &Path,
    repo: &str,
    default_branch: &str,
) -> Result<Vec<WorktreeStatus>> {
    let origin_ref = format!("origin/{default_branch}");
    let mut out = Vec::new();
    for (agent, branch) in AGENTS {
        let wt = worktree_dir(projects_dir, repo, agent);
        if !wt.exists() {
            continue;
        }
        let st = worktree_status_in(&wt, &origin_ref).await?;
        out.push(WorktreeStatus {
            agent: (*agent).into(),
            branch: (*branch).into(),
            ahead: st.ahead,
            behind: st.behind,
            dirty: st.dirty,
            diverged: st.diverged,
        });
    }
    Ok(out)
}

#[derive(Debug)]
struct WtCheck {
    dirty: bool,
    diverged: bool,
    ahead: u32,
    behind: u32,
}

async fn worktree_status_in(wt: &Path, origin_ref: &str) -> Result<WtCheck> {
    let dirty = is_dirty(wt).await?;
    let (ahead, behind) = ahead_behind(wt, origin_ref).await.unwrap_or((0, 0));
    // Diverged: both ahead and behind relative to origin/default.
    let diverged = ahead > 0 && behind > 0;
    Ok(WtCheck {
        dirty,
        diverged,
        ahead,
        behind,
    })
}

async fn is_dirty(wt: &Path) -> Result<bool> {
    let out = shell::run_git(&["status", "--porcelain"], Some(wt), &HashMap::new()).await?;
    out.ensure_success("git status")?;
    Ok(!out.stdout.trim().is_empty())
}

async fn ahead_behind(wt: &Path, upstream: &str) -> Result<(u32, u32)> {
    // Ensure upstream exists.
    let verify = shell::run_git(
        &["rev-parse", "--verify", upstream],
        Some(wt),
        &HashMap::new(),
    )
    .await?;
    if !verify.success() {
        return Ok((0, 0));
    }
    let out = shell::run_git(
        &["rev-list", "--left-right", "--count", &format!("HEAD...{upstream}")],
        Some(wt),
        &HashMap::new(),
    )
    .await?;
    out.ensure_success("git rev-list")?;
    parse_left_right(&out.stdout)
}

fn parse_left_right(s: &str) -> Result<(u32, u32)> {
    let mut parts = s.split_whitespace();
    let left: u32 = parts
        .next()
        .ok_or_else(|| anyhow!("rev-list empty"))?
        .parse()?;
    let right: u32 = parts
        .next()
        .ok_or_else(|| anyhow!("rev-list missing behind"))?
        .parse()?;
    Ok((left, right))
}

async fn rev_parse(cwd: &Path, rev: &str) -> Result<String> {
    let out: CmdOutput = shell::run_git(&["rev-parse", rev], Some(cwd), &HashMap::new()).await?;
    out.ensure_success("git rev-parse")?;
    Ok(out.stdout.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn auth_debug_display_hide_token() {
        let auth = GitHttpsAuth::new("ghp_SuperSecretToken99");
        let dbg = format!("{auth:?}");
        let disp = format!("{auth}");
        assert!(!dbg.contains("SuperSecretToken99"));
        assert!(!disp.contains("SuperSecretToken99"));
        assert!(dbg.contains("[redacted]"));
        assert!(!auth.basic_authorization_header().contains("SuperSecretToken99"));
        // Header is Basic base64 — still must not appear in Debug/Display.
        let b64 = auth
            .basic_authorization_header()
            .strip_prefix("Authorization: Basic ")
            .unwrap()
            .to_string();
        assert!(!dbg.contains(&b64));
        assert!(!disp.contains(&b64));
    }

    #[test]
    fn auth_env_uses_basic_not_url_token() {
        let auth = GitHttpsAuth::new("ghp_EnvSecretToken42");
        let env = auth.env();
        assert_eq!(env.get("GIT_TERMINAL_PROMPT").map(String::as_str), Some("0"));
        assert_eq!(env.get("GIT_CONFIG_COUNT").map(String::as_str), Some("2"));
        let header = env.get("GIT_CONFIG_VALUE_0").unwrap();
        assert!(header.starts_with("Authorization: Basic "));
        assert!(!header.contains("ghp_EnvSecretToken42"));
        // credential.helper cleared so global `store` cannot force a TTY prompt.
        assert_eq!(env.get("GIT_CONFIG_KEY_1").map(String::as_str), Some("credential.helper"));
        assert_eq!(env.get("GIT_CONFIG_VALUE_1").map(String::as_str), Some(""));
    }

    #[test]
    fn auth_failure_classifier() {
        assert!(is_auth_failure(
            "fatal: could not read Username for 'https://github.com': No such device or address"
        ));
        assert!(is_auth_failure("remote: Invalid username or password."));
        assert!(!is_auth_failure("fatal: not a git repository"));
    }

    #[test]
    fn auth_redacts_basic_blob_in_errors() {
        let auth = GitHttpsAuth::new("opaque-token-value");
        let header = auth.basic_authorization_header();
        let b64 = header.strip_prefix("Authorization: Basic ").unwrap();
        let out = CmdOutput {
            status: 128,
            stdout: String::new(),
            stderr: format!("fatal: {header} rejected"),
        };
        let err = git_err("git clone --bare", &out, &auth).to_string();
        assert!(!err.contains("opaque-token-value"));
        assert!(!err.contains(b64));
        assert!(err.contains("authentication") || err.contains("Authorization"));
    }

    async fn git(args: &[&str], cwd: Option<&Path>) -> CmdOutput {
        shell::run_git(args, cwd, &HashMap::new()).await.unwrap()
    }

    async fn init_upstream(dir: &Path) -> String {
        tokio::fs::create_dir_all(dir).await.unwrap();
        git(&["init", "-b", "main"], Some(dir)).await.ensure_success("init").unwrap();
        git(&["config", "user.email", "test@example.com"], Some(dir))
            .await
            .ensure_success("email")
            .unwrap();
        git(&["config", "user.name", "Test"], Some(dir))
            .await
            .ensure_success("name")
            .unwrap();
        let file = dir.join("README.md");
        tokio::fs::write(&file, "v1\n").await.unwrap();
        git(&["add", "README.md"], Some(dir))
            .await
            .ensure_success("add")
            .unwrap();
        git(&["commit", "-m", "init"], Some(dir))
            .await
            .ensure_success("commit")
            .unwrap();
        // bare remote
        let bare = dir.parent().unwrap().join("upstream.git");
        git(&["clone", "--bare", &dir.to_string_lossy(), &bare.to_string_lossy()], None)
            .await
            .ensure_success("bare")
            .unwrap();
        bare.to_string_lossy().into_owned()
    }

    #[tokio::test]
    async fn sync_creates_bare_and_worktrees() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let remote = init_upstream(&src).await;
        let projects = tmp.path().join("projects");

        let (created, _fetched, results) =
            sync_project(&projects, "demo", &remote, "main", "unused").await.unwrap();
        assert!(created);
        assert!(bare_dir(&projects, "demo").exists());
        assert!(worktree_dir(&projects, "demo", "cursor").exists());
        assert!(worktree_dir(&projects, "demo", "opencode").exists());
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.action == WorktreeAction::Created));

        // Idempotent re-sync
        let (created2, fetched2, results2) =
            sync_project(&projects, "demo", &remote, "main", "unused").await.unwrap();
        assert!(!created2);
        assert!(fetched2);
        assert!(results2
            .iter()
            .all(|r| r.action == WorktreeAction::AlreadyUpToDate));
    }

    /// Restart simulation: disk inventory must not depend on tmux/process state.
    #[tokio::test]
    async fn list_on_disk_projects_survives_without_tmux() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let remote = init_upstream(&src).await;
        let projects = tmp.path().join("projects");

        sync_project(&projects, "demo", &remote, "main", "unused")
            .await
            .unwrap();
        sync_project(&projects, "other", &remote, "main", "unused")
            .await
            .unwrap();

        // Drop a junk dir without .bare — must be ignored.
        tokio::fs::create_dir_all(projects.join("not-a-project"))
            .await
            .unwrap();

        let names = list_on_disk_projects(&projects).await.unwrap();
        assert_eq!(names, vec!["demo".to_string(), "other".to_string()]);

        let statuses = project_worktree_statuses(&projects, "demo", "main")
            .await
            .unwrap();
        assert_eq!(statuses.len(), 2);
        assert!(statuses.iter().all(|s| !s.dirty));
    }

    #[tokio::test]
    async fn sync_skips_dirty_worktree() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let remote = init_upstream(&src).await;
        let projects = tmp.path().join("projects");

        sync_project(&projects, "demo", &remote, "main", "unused")
            .await
            .unwrap();

        let cursor = worktree_dir(&projects, "demo", "cursor");
        tokio::fs::write(cursor.join("dirty.txt"), "nope").await.unwrap();

        let (_c, _f, results) =
            sync_project(&projects, "demo", &remote, "main", "unused").await.unwrap();
        let cursor_r = results.iter().find(|r| r.agent == "cursor").unwrap();
        assert_eq!(cursor_r.action, WorktreeAction::SkippedDirty);
        assert!(cursor_r.dirty);
        // dirty file still there
        assert!(cursor.join("dirty.txt").exists());
    }

    #[tokio::test]
    async fn sync_skips_diverged_worktree() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let remote = init_upstream(&src).await;
        let projects = tmp.path().join("projects");

        sync_project(&projects, "demo", &remote, "main", "unused")
            .await
            .unwrap();

        let cursor = worktree_dir(&projects, "demo", "cursor");
        git(&["config", "user.email", "test@example.com"], Some(&cursor))
            .await
            .ensure_success("email")
            .unwrap();
        git(&["config", "user.name", "Test"], Some(&cursor))
            .await
            .ensure_success("name")
            .unwrap();
        tokio::fs::write(cursor.join("local.txt"), "mine\n").await.unwrap();
        git(&["add", "local.txt"], Some(&cursor))
            .await
            .ensure_success("add local")
            .unwrap();
        git(&["commit", "-m", "local"], Some(&cursor))
            .await
            .ensure_success("commit local")
            .unwrap();

        // Advance upstream so cursor is both ahead and behind.
        let work = tmp.path().join("upstream-work");
        git(&["clone", &remote, &work.to_string_lossy()], None)
            .await
            .ensure_success("clone upstream work")
            .unwrap();
        git(&["config", "user.email", "test@example.com"], Some(&work))
            .await
            .ensure_success("email")
            .unwrap();
        git(&["config", "user.name", "Test"], Some(&work))
            .await
            .ensure_success("name")
            .unwrap();
        tokio::fs::write(work.join("README.md"), "v2\n").await.unwrap();
        git(&["add", "README.md"], Some(&work))
            .await
            .ensure_success("add remote")
            .unwrap();
        git(&["commit", "-m", "remote"], Some(&work))
            .await
            .ensure_success("commit remote")
            .unwrap();
        git(&["push", "origin", "main"], Some(&work))
            .await
            .ensure_success("push")
            .unwrap();

        let (_c, _f, results) =
            sync_project(&projects, "demo", &remote, "main", "unused").await.unwrap();
        let cursor_r = results.iter().find(|r| r.agent == "cursor").unwrap();
        assert_eq!(cursor_r.action, WorktreeAction::SkippedDiverged);
        assert!(cursor_r.diverged);
        assert!(cursor.join("local.txt").exists());
    }
}
