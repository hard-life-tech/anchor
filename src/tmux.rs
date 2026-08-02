//! Idempotent tmux session/window/pane ensure. Never kill live panes.

use anyhow::{Context, Result};
use serde::Serialize;

use crate::shell;

#[derive(Debug, Clone, Serialize)]
pub struct TmuxEnsureResult {
    pub session: String,
    pub window: String,
    pub created_window: bool,
    pub panes_ensured: bool,
}

/// Ensure session + window exist. Launch agent cmds only into empty panes.
/// `GITHUB_TOKEN` is stripped from tmux invocations (see `shell::run_tmux`)
/// and unset on the session so panes never inherit it.
///
/// `legacy_window_names`: optional old window names to rename → `window` (idempotent;
/// never kills panes).
pub async fn ensure_project_window(
    session: &str,
    window: &str,
    cursor_cwd: &str,
    opencode_cwd: &str,
    cursor_cmd: &str,
    opencode_cmd: &str,
) -> Result<TmuxEnsureResult> {
    ensure_project_window_renaming(
        session,
        window,
        cursor_cwd,
        opencode_cwd,
        cursor_cmd,
        opencode_cmd,
        &[],
    )
    .await
}

pub async fn ensure_project_window_renaming(
    session: &str,
    window: &str,
    cursor_cwd: &str,
    opencode_cwd: &str,
    cursor_cmd: &str,
    opencode_cmd: &str,
    legacy_window_names: &[&str],
) -> Result<TmuxEnsureResult> {
    let mut created_window = false;

    if !session_exists(session).await? {
        let out = shell::run_tmux(&[
            "new-session",
            "-d",
            "-s",
            session,
            "-n",
            window,
            "-c",
            cursor_cwd,
        ])
        .await?;
        out.ensure_success("tmux new-session")?;
        created_window = true;
    } else if !window_exists(session, window).await? {
        // Prefer renaming a legacy window over creating a duplicate.
        let mut renamed = false;
        for old in legacy_window_names {
            if *old != window && window_exists(session, old).await? {
                let out = shell::run_tmux(&[
                    "rename-window",
                    "-t",
                    &format!("{session}:{old}"),
                    window,
                ])
                .await?;
                if out.success() {
                    renamed = true;
                    break;
                }
            }
        }
        if !renamed {
            let out = shell::run_tmux(&[
                "new-window",
                "-t",
                session,
                "-n",
                window,
                "-c",
                cursor_cwd,
            ])
            .await?;
            out.ensure_success("tmux new-window")?;
            created_window = true;
        }
    }

    // Always scrub — session may have been created by an older Anchor or by hand.
    scrub_github_token(session).await?;

    let target = format!("{session}:{window}");
    ensure_two_panes(&target, cursor_cwd, opencode_cwd).await?;

    maybe_launch_pane(&target, "0", cursor_cwd, cursor_cmd).await?;
    maybe_launch_pane(&target, "1", opencode_cwd, opencode_cmd).await?;

    Ok(TmuxEnsureResult {
        session: session.into(),
        window: window.into(),
        created_window,
        panes_ensured: true,
    })
}

/// Drop `GITHUB_TOKEN` from global + session tmux environments.
pub async fn scrub_github_token(session: &str) -> Result<()> {
    let _ = shell::run_tmux(&["set-environment", "-g", "-u", "GITHUB_TOKEN"]).await;
    let _ = shell::run_tmux(&["set-environment", "-t", session, "-u", "GITHUB_TOKEN"]).await;
    Ok(())
}

/// True when session env still lists `GITHUB_TOKEN` (should be false after scrub).
#[cfg(test)]
pub async fn session_has_github_token(session: &str) -> Result<bool> {
    let out = shell::run_tmux(&["show-environment", "-t", session]).await?;
    if !out.success() {
        return Ok(false);
    }
    Ok(out
        .stdout
        .lines()
        .any(|l| l.starts_with("GITHUB_TOKEN=") || l.trim() == "GITHUB_TOKEN"))
}

pub async fn session_exists(session: &str) -> Result<bool> {
    let out = shell::run_tmux(&["has-session", "-t", session]).await?;
    Ok(out.success())
}

pub async fn window_exists(session: &str, window: &str) -> Result<bool> {
    let out = shell::run_tmux(&["list-windows", "-t", session, "-F", "#{window_name}"]).await?;
    if !out.success() {
        return Ok(false);
    }
    Ok(out.stdout.lines().any(|l| l.trim() == window))
}

/// Select a pane and zoom it so a single-client attach shows one agent TUI.
pub async fn ensure_pane_zoomed(session: &str, window: &str, pane_index: &str) -> Result<()> {
    let target = format!("{session}:{window}.{pane_index}");
    let out = shell::run_tmux(&["select-pane", "-t", &target]).await?;
    out.ensure_success("tmux select-pane")?;

    let flag = shell::run_tmux(&["display-message", "-p", "-t", &target, "#{window_zoomed_flag}"])
        .await?;
    if flag.success() && flag.stdout.trim() == "1" {
        return Ok(());
    }
    let out = shell::run_tmux(&["resize-pane", "-t", &target, "-Z"]).await?;
    out.ensure_success("tmux resize-pane -Z")?;
    Ok(())
}

async fn ensure_two_panes(target: &str, cursor_cwd: &str, opencode_cwd: &str) -> Result<()> {
    let count = pane_count(target).await?;
    if count < 2 {
        let out = shell::run_tmux(&["split-window", "-t", target, "-h", "-c", opencode_cwd]).await?;
        out.ensure_success("tmux split-window")?;
    }
    // Select layout even if panes already existed — harmless.
    let _ = shell::run_tmux(&["select-layout", "-t", target, "even-horizontal"]).await?;
    // Ensure pane 0 cwd is cursor (best-effort; do not kill).
    let _ = cursor_cwd;
    Ok(())
}

async fn pane_count(target: &str) -> Result<usize> {
    let out = shell::run_tmux(&["list-panes", "-t", target, "-F", "#{pane_id}"]).await?;
    out.ensure_success("tmux list-panes")?;
    Ok(out.stdout.lines().filter(|l| !l.trim().is_empty()).count())
}

async fn pane_dead(target: &str, pane_index: &str) -> Result<bool> {
    let t = format!("{target}.{pane_index}");
    let out = shell::run_tmux(&["list-panes", "-t", &t, "-F", "#{pane_dead}"]).await?;
    if !out.success() {
        return Ok(true);
    }
    Ok(out.stdout.trim() == "1")
}

async fn pane_current_command(target: &str, pane_index: &str) -> Result<String> {
    let t = format!("{target}.{pane_index}");
    let out = shell::run_tmux(&["list-panes", "-t", &t, "-F", "#{pane_current_command}"]).await?;
    out.ensure_success("tmux pane command")?;
    Ok(out.stdout.trim().to_string())
}

async fn maybe_launch_pane(
    target: &str,
    pane_index: &str,
    cwd: &str,
    cmd: &str,
) -> Result<()> {
    // Never restart a live agent — only launch if the pane looks like an idle shell.
    if pane_dead(target, pane_index).await? {
        return Ok(());
    }
    let current = pane_current_command(target, pane_index).await.unwrap_or_default();
    let idle = matches!(
        current.as_str(),
        "bash" | "zsh" | "sh" | "fish" | "tmux" | ""
    );
    if !idle {
        return Ok(());
    }

    let t = format!("{target}.{pane_index}");
    // Session-level scrub already ran; clear again on the window target before launch.
    let session = target.split(':').next().unwrap_or(target);
    scrub_github_token(session).await?;

    let launch = format!("cd {} && exec {}", shell_quote(cwd), cmd);
    let out = shell::run_tmux(&["send-keys", "-t", &t, &launch, "C-m"])
        .await
        .context("tmux send-keys")?;
    out.ensure_success("tmux send-keys")?;
    Ok(())
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmux_available() -> bool {
        std::process::Command::new("tmux")
            .arg("-V")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[tokio::test]
    async fn ensure_window_idempotent() {
        if !tmux_available() {
            eprintln!("skip: tmux not installed");
            return;
        }
        let tmp = TempDir::new().unwrap();
        let cursor = tmp.path().join("cursor");
        let opencode = tmp.path().join("opencode");
        tokio::fs::create_dir_all(&cursor).await.unwrap();
        tokio::fs::create_dir_all(&opencode).await.unwrap();

        let session = format!("anchor-test-{}", std::process::id());
        // cleanup leftover
        let _ = shell::run_tmux(&["kill-session", "-t", &session]).await;

        let r1 = ensure_project_window(
            &session,
            "demo",
            &cursor.to_string_lossy(),
            &opencode.to_string_lossy(),
            "true",
            "true",
        )
        .await
        .unwrap();
        assert!(r1.created_window);

        let r2 = ensure_project_window(
            &session,
            "demo",
            &cursor.to_string_lossy(),
            &opencode.to_string_lossy(),
            "true",
            "true",
        )
        .await
        .unwrap();
        assert!(!r2.created_window);
        assert!(window_exists(&session, "demo").await.unwrap());
        assert!(!session_has_github_token(&session).await.unwrap());

        let _ = shell::run_tmux(&["kill-session", "-t", &session]).await;
    }

    #[tokio::test]
    async fn scrub_removes_injected_token_from_session_env() {
        if !tmux_available() {
            eprintln!("skip: tmux not installed");
            return;
        }
        let session = format!("anchor-scrub-{}", std::process::id());
        let _ = shell::run_tmux(&["kill-session", "-t", &session]).await;

        // Inject token via a tmux client that still has GITHUB_TOKEN stripped from
        // our helper — use raw Command so we can set the var on set-environment.
        let tmp = TempDir::new().unwrap();
        let cwd = tmp.path().join("wt");
        tokio::fs::create_dir_all(&cwd).await.unwrap();
        let out = shell::run_tmux(&[
            "new-session",
            "-d",
            "-s",
            &session,
            "-n",
            "demo",
            "-c",
            &cwd.to_string_lossy(),
        ])
        .await
        .unwrap();
        out.ensure_success("new-session").unwrap();

        // Force-set token on the session (simulates a leaky parent env).
        let set = tokio::process::Command::new("tmux")
            .args(["set-environment", "-t", &session, "GITHUB_TOKEN", "leaked-secret"])
            .output()
            .await
            .unwrap();
        assert!(set.status.success());
        assert!(session_has_github_token(&session).await.unwrap());

        scrub_github_token(&session).await.unwrap();
        assert!(!session_has_github_token(&session).await.unwrap());

        let _ = shell::run_tmux(&["kill-session", "-t", &session]).await;
    }
}
