//! Bare clone + worktree sync via shell-outs to `git`.
//!
//! Multi-repo sibling layout (ADR-0010):
//! ```text
//! $PROJECTS_DIR/<slug>/
//!   .anchor/project.json
//!   .bares/<owner>__<repo>/
//!   cursor/<owner>__<repo>/
//!   opencode/<owner>__<repo>/
//! ```

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

/// Owner-scoped filesystem key (avoids short-name collisions).
pub fn repo_key(owner: &str, name: &str) -> String {
    format!("{owner}__{name}")
}

pub fn project_dir(projects_dir: &Path, slug: &str) -> PathBuf {
    projects_dir.join(slug)
}

pub fn agent_workspace(projects_dir: &Path, slug: &str, agent: &str) -> PathBuf {
    project_dir(projects_dir, slug).join(agent)
}

pub fn bare_dir(projects_dir: &Path, slug: &str, owner: &str, name: &str) -> PathBuf {
    project_dir(projects_dir, slug)
        .join(".bares")
        .join(repo_key(owner, name))
}

pub fn worktree_dir(
    projects_dir: &Path,
    slug: &str,
    agent: &str,
    owner: &str,
    name: &str,
) -> PathBuf {
    agent_workspace(projects_dir, slug, agent).join(repo_key(owner, name))
}

/// Legacy 1:1 layout: `$PROJECTS_DIR/<name>/.bare` (not yet migrated).
pub fn is_legacy_layout(dir: &Path) -> bool {
    dir.join(".bare").is_dir() && !dir.join(".bares").is_dir() && !dir.join(".anchor").join("project.json").is_file()
}

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

/// Sync one member repo into a project. Idempotent; never force-overwrite dirty trees.
///
/// `clone_url` must be a normal HTTPS/SSH URL **without** embedded credentials.
pub async fn sync_member(
    projects_dir: &Path,
    slug: &str,
    owner: &str,
    name: &str,
    clone_url: &str,
    default_branch: &str,
    token: &str,
) -> Result<(bool, bool, Vec<WorktreeResult>)> {
    let bare = bare_dir(projects_dir, slug, owner, name);
    let auth = GitHttpsAuth::new(token);
    let env = auth.env();
    let mut created = false;

    let proj = project_dir(projects_dir, slug);
    tokio::fs::create_dir_all(proj.join(".bares"))
        .await
        .context("create .bares")?;
    for (agent, _) in AGENTS {
        tokio::fs::create_dir_all(agent_workspace(projects_dir, slug, agent))
            .await
            .with_context(|| format!("create {agent} workspace"))?;
    }

    if !bare.exists() {
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

    let origin_ref = format!("origin/{default_branch}");
    ensure_origin_ref(&bare, default_branch, &origin_ref).await?;
    let mut results = Vec::new();

    for (agent, branch) in AGENTS {
        let wt = worktree_dir(projects_dir, slug, agent, owner, name);
        if !wt.exists() {
            let exists = shell::run_git(
                &[
                    "show-ref",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/{branch}"),
                ],
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
                &["worktree", "add", &wt.to_string_lossy(), branch],
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
        let out =
            shell::run_git(&["merge", "--ff-only", &origin_ref], Some(&wt), &HashMap::new())
                .await?;
        if !out.success() {
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

/// Migrate legacy `$PROJECTS_DIR/<shortname>/{.bare,cursor,opencode}` into sibling layout.
///
/// Returns `true` if a migration was performed.
pub async fn migrate_legacy_project(
    projects_dir: &Path,
    shortname: &str,
    owner: &str,
) -> Result<bool> {
    let dir = project_dir(projects_dir, shortname);
    if !is_legacy_layout(&dir) {
        return Ok(false);
    }

    let key = repo_key(owner, shortname);
    let old_bare = dir.join(".bare");
    let bares = dir.join(".bares");
    let new_bare = bares.join(&key);

    tokio::fs::create_dir_all(&bares)
        .await
        .context("create .bares for migration")?;

    // Relocate worktrees out from under agent roots before nesting.
    for agent in ["cursor", "opencode"] {
        let old_wt = dir.join(agent);
        if !old_wt.is_dir() {
            continue;
        }
        // If already nested (owner__repo), skip.
        if old_wt.join(&key).exists() {
            continue;
        }
        // Detect legacy worktree: contains .git file (linked worktree).
        let gitlink = old_wt.join(".git");
        if !gitlink.exists() {
            continue;
        }

        let staging = dir.join(format!(".__migrate_{agent}"));
        if staging.exists() {
            tokio::fs::remove_dir_all(&staging).await.ok();
        }
        tokio::fs::rename(&old_wt, &staging)
            .await
            .with_context(|| format!("stage {agent} worktree"))?;
        tokio::fs::create_dir_all(&old_wt)
            .await
            .with_context(|| format!("recreate {agent}/"))?;
        let dest = old_wt.join(&key);
        tokio::fs::rename(&staging, &dest)
            .await
            .with_context(|| format!("nest {agent} worktree under {key}"))?;
    }

    // Move bare clone.
    if old_bare.exists() && !new_bare.exists() {
        tokio::fs::rename(&old_bare, &new_bare)
            .await
            .context("move .bare into .bares")?;
    }

    // Repair worktree ↔ bare links after the moves.
    let mut repair_args = vec!["worktree".to_string(), "repair".to_string()];
    for agent in ["cursor", "opencode"] {
        let wt = dir.join(agent).join(&key);
        if wt.exists() {
            repair_args.push(wt.to_string_lossy().into_owned());
        }
    }
    if repair_args.len() > 2 {
        let args: Vec<&str> = repair_args.iter().map(String::as_str).collect();
        let out = shell::run_git(&args, Some(&new_bare), &HashMap::new()).await?;
        // Best-effort — older git may lack repair; still usable if paths were absolute-correct.
        if !out.success() {
            tracing::warn!(
                slug = shortname,
                "git worktree repair after migration: {}",
                redact_secrets(out.stderr.trim())
            );
        }
    }

    Ok(true)
}

/// Scan `PROJECTS_DIR` for legacy layouts and migrate them.
pub async fn migrate_all_legacy(projects_dir: &Path, default_owner: &str) -> Result<Vec<String>> {
    let mut migrated = Vec::new();
    if !projects_dir.exists() {
        return Ok(migrated);
    }
    let mut entries = tokio::fs::read_dir(projects_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if migrate_legacy_project(projects_dir, &name, default_owner).await? {
            migrated.push(name);
        }
    }
    migrated.sort();
    Ok(migrated)
}

pub async fn list_on_disk_slugs(projects_dir: &Path) -> Result<Vec<String>> {
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
        let dir = entry.path();
        let has_meta = dir.join(".anchor").join("project.json").is_file();
        let has_bares = dir.join(".bares").is_dir();
        let legacy = is_legacy_layout(&dir);
        if has_meta || has_bares || legacy {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

pub async fn member_worktree_statuses(
    projects_dir: &Path,
    slug: &str,
    owner: &str,
    name: &str,
    default_branch: &str,
) -> Result<Vec<WorktreeStatus>> {
    let origin_ref = format!("origin/{default_branch}");
    let mut out = Vec::new();
    for (agent, branch) in AGENTS {
        let wt = worktree_dir(projects_dir, slug, agent, owner, name);
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

/// Remove member bare + worktrees when clean. Returns Err if dirty (do not force).
pub async fn remove_member_disk(
    projects_dir: &Path,
    slug: &str,
    owner: &str,
    name: &str,
    default_branch: &str,
) -> Result<()> {
    let statuses =
        member_worktree_statuses(projects_dir, slug, owner, name, default_branch).await?;
    if statuses.iter().any(|s| s.dirty) {
        return Err(anyhow!(
            "cannot remove {owner}/{name}: dirty worktree(s) present"
        ));
    }

    let bare = bare_dir(projects_dir, slug, owner, name);
    for (agent, _) in AGENTS {
        let wt = worktree_dir(projects_dir, slug, agent, owner, name);
        if wt.exists() && bare.exists() {
            let out = shell::run_git(
                &["worktree", "remove", "--force", &wt.to_string_lossy()],
                Some(&bare),
                &HashMap::new(),
            )
            .await?;
            if !out.success() {
                // Fall back to plain remove if worktree unregister fails.
                tokio::fs::remove_dir_all(&wt).await.ok();
            }
        } else if wt.exists() {
            tokio::fs::remove_dir_all(&wt).await?;
        }
    }
    if bare.exists() {
        tokio::fs::remove_dir_all(&bare).await?;
    }
    Ok(())
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
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("HEAD...{upstream}"),
        ],
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
    fn repo_key_owner_scoped() {
        assert_eq!(repo_key("alice", "anchor"), "alice__anchor");
        assert_eq!(
            bare_dir(Path::new("/p"), "plat", "alice", "anchor"),
            PathBuf::from("/p/plat/.bares/alice__anchor")
        );
        assert_eq!(
            worktree_dir(Path::new("/p"), "plat", "cursor", "alice", "anchor"),
            PathBuf::from("/p/plat/cursor/alice__anchor")
        );
    }

    #[test]
    fn auth_debug_display_hide_token() {
        let auth = GitHttpsAuth::new("ghp_SuperSecretToken99");
        let dbg = format!("{auth:?}");
        let disp = format!("{auth}");
        assert!(!dbg.contains("SuperSecretToken99"));
        assert!(!disp.contains("SuperSecretToken99"));
        assert!(dbg.contains("[redacted]"));
        assert!(!auth
            .basic_authorization_header()
            .contains("SuperSecretToken99"));
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
        assert_eq!(
            env.get("GIT_TERMINAL_PROMPT").map(String::as_str),
            Some("0")
        );
        assert_eq!(env.get("GIT_CONFIG_COUNT").map(String::as_str), Some("2"));
        let header = env.get("GIT_CONFIG_VALUE_0").unwrap();
        assert!(header.starts_with("Authorization: Basic "));
        assert!(!header.contains("ghp_EnvSecretToken42"));
        assert_eq!(
            env.get("GIT_CONFIG_KEY_1").map(String::as_str),
            Some("credential.helper")
        );
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
        git(&["init", "-b", "main"], Some(dir))
            .await
            .ensure_success("init")
            .unwrap();
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
        let bare = dir.parent().unwrap().join("upstream.git");
        git(
            &[
                "clone",
                "--bare",
                &dir.to_string_lossy(),
                &bare.to_string_lossy(),
            ],
            None,
        )
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

        let (created, _fetched, results) = sync_member(
            &projects,
            "platform",
            "alice",
            "demo",
            &remote,
            "main",
            "unused",
        )
        .await
        .unwrap();
        assert!(created);
        assert!(bare_dir(&projects, "platform", "alice", "demo").exists());
        assert!(worktree_dir(&projects, "platform", "cursor", "alice", "demo").exists());
        assert!(worktree_dir(&projects, "platform", "opencode", "alice", "demo").exists());
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.action == WorktreeAction::Created));

        let (created2, fetched2, results2) = sync_member(
            &projects,
            "platform",
            "alice",
            "demo",
            &remote,
            "main",
            "unused",
        )
        .await
        .unwrap();
        assert!(!created2);
        assert!(fetched2);
        assert!(results2
            .iter()
            .all(|r| r.action == WorktreeAction::AlreadyUpToDate));
    }

    #[tokio::test]
    async fn sync_two_members_under_one_slug() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let remote = init_upstream(&src).await;
        let projects = tmp.path().join("projects");

        sync_member(
            &projects,
            "platform",
            "alice",
            "demo",
            &remote,
            "main",
            "unused",
        )
        .await
        .unwrap();
        sync_member(
            &projects,
            "platform",
            "alice",
            "other",
            &remote,
            "main",
            "unused",
        )
        .await
        .unwrap();

        assert!(worktree_dir(&projects, "platform", "cursor", "alice", "demo").exists());
        assert!(worktree_dir(&projects, "platform", "cursor", "alice", "other").exists());
        assert_eq!(
            agent_workspace(&projects, "platform", "cursor"),
            projects.join("platform/cursor")
        );
    }

    #[tokio::test]
    async fn list_on_disk_after_sync() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let remote = init_upstream(&src).await;
        let projects = tmp.path().join("projects");

        sync_member(
            &projects,
            "demo",
            "alice",
            "demo",
            &remote,
            "main",
            "unused",
        )
        .await
        .unwrap();
        // Write a minimal project.json so inventory treats it as a project.
        tokio::fs::create_dir_all(projects.join("demo/.anchor"))
            .await
            .unwrap();
        tokio::fs::write(
            projects.join("demo/.anchor/project.json"),
            r#"{"id":"1","slug":"demo","name":"Demo","members":[]}"#,
        )
        .await
        .unwrap();

        tokio::fs::create_dir_all(projects.join("not-a-project"))
            .await
            .unwrap();

        let names = list_on_disk_slugs(&projects).await.unwrap();
        assert_eq!(names, vec!["demo".to_string()]);
    }

    #[tokio::test]
    async fn migrate_legacy_layout() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let remote = init_upstream(&src).await;
        let projects = tmp.path().join("projects");

        // Build legacy layout manually (old sync shape).
        let legacy = projects.join("demo");
        tokio::fs::create_dir_all(&legacy).await.unwrap();
        let bare = legacy.join(".bare");
        git(
            &[
                "clone",
                "--bare",
                &remote,
                &bare.to_string_lossy(),
            ],
            None,
        )
        .await
        .ensure_success("legacy bare")
        .unwrap();
        ensure_origin_fetch_config(&bare).await.unwrap();
        git(&["fetch", "origin"], Some(&bare))
            .await
            .ensure_success("fetch")
            .unwrap();
        ensure_origin_ref(&bare, "main", "origin/main")
            .await
            .unwrap();
        git(&["branch", "agent/cursor", "origin/main"], Some(&bare))
            .await
            .ensure_success("branch cursor")
            .unwrap();
        git(&["branch", "agent/opencode", "origin/main"], Some(&bare))
            .await
            .ensure_success("branch opencode")
            .unwrap();
        git(
            &[
                "worktree",
                "add",
                &legacy.join("cursor").to_string_lossy(),
                "agent/cursor",
            ],
            Some(&bare),
        )
        .await
        .ensure_success("wt cursor")
        .unwrap();
        git(
            &[
                "worktree",
                "add",
                &legacy.join("opencode").to_string_lossy(),
                "agent/opencode",
            ],
            Some(&bare),
        )
        .await
        .ensure_success("wt opencode")
        .unwrap();

        assert!(is_legacy_layout(&legacy));
        assert!(migrate_legacy_project(&projects, "demo", "alice")
            .await
            .unwrap());
        assert!(!is_legacy_layout(&legacy));
        assert!(bare_dir(&projects, "demo", "alice", "demo").exists());
        assert!(worktree_dir(&projects, "demo", "cursor", "alice", "demo").exists());
        assert!(worktree_dir(&projects, "demo", "opencode", "alice", "demo").exists());
        // Idempotent.
        assert!(!migrate_legacy_project(&projects, "demo", "alice")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn sync_skips_dirty_worktree() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let remote = init_upstream(&src).await;
        let projects = tmp.path().join("projects");

        sync_member(
            &projects,
            "demo",
            "alice",
            "demo",
            &remote,
            "main",
            "unused",
        )
        .await
        .unwrap();

        let cursor = worktree_dir(&projects, "demo", "cursor", "alice", "demo");
        tokio::fs::write(cursor.join("dirty.txt"), "nope")
            .await
            .unwrap();

        let (_c, _f, results) = sync_member(
            &projects,
            "demo",
            "alice",
            "demo",
            &remote,
            "main",
            "unused",
        )
        .await
        .unwrap();
        let cursor_r = results.iter().find(|r| r.agent == "cursor").unwrap();
        assert_eq!(cursor_r.action, WorktreeAction::SkippedDirty);
        assert!(cursor_r.dirty);
        assert!(cursor.join("dirty.txt").exists());
    }

    #[tokio::test]
    async fn sync_skips_diverged_worktree() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let remote = init_upstream(&src).await;
        let projects = tmp.path().join("projects");

        sync_member(
            &projects,
            "demo",
            "alice",
            "demo",
            &remote,
            "main",
            "unused",
        )
        .await
        .unwrap();

        let cursor = worktree_dir(&projects, "demo", "cursor", "alice", "demo");
        git(&["config", "user.email", "test@example.com"], Some(&cursor))
            .await
            .ensure_success("email")
            .unwrap();
        git(&["config", "user.name", "Test"], Some(&cursor))
            .await
            .ensure_success("name")
            .unwrap();
        tokio::fs::write(cursor.join("local.txt"), "mine\n")
            .await
            .unwrap();
        git(&["add", "local.txt"], Some(&cursor))
            .await
            .ensure_success("add local")
            .unwrap();
        git(&["commit", "-m", "local"], Some(&cursor))
            .await
            .ensure_success("commit local")
            .unwrap();

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

        let (_c, _f, results) = sync_member(
            &projects,
            "demo",
            "alice",
            "demo",
            &remote,
            "main",
            "unused",
        )
        .await
        .unwrap();
        let cursor_r = results.iter().find(|r| r.agent == "cursor").unwrap();
        assert_eq!(cursor_r.action, WorktreeAction::SkippedDiverged);
        assert!(cursor_r.diverged);
        assert!(cursor.join("local.txt").exists());
    }
}
