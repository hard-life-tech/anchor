//! Centralized process spawning for git/tmux (no ad-hoc Command::new elsewhere).

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Stdio;

use anyhow::{anyhow, Context, Result};
use tokio::process::Command;

#[derive(Debug, Clone, Default)]
pub struct CmdOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CmdOutput {
    pub fn success(&self) -> bool {
        self.status == 0
    }

    pub fn ensure_success(&self, label: &str) -> Result<()> {
        if self.success() {
            Ok(())
        } else {
            // Scrub PAT shapes from stderr before they enter anyhow/API/logs.
            let stderr = crate::error::redact_secrets(self.stderr.trim());
            Err(anyhow!(
                "{label} failed (exit {}): {stderr}",
                self.status
            ))
        }
    }
}

/// Run a command asynchronously. Extra env vars are merged; use `clear_env_keys` to drop secrets.
pub async fn run(
    program: impl AsRef<OsStr>,
    args: &[&str],
    cwd: Option<&Path>,
    extra_env: &HashMap<String, String>,
    clear_env_keys: &[&str],
) -> Result<CmdOutput> {
    let mut cmd = Command::new(program.as_ref());
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    for key in clear_env_keys {
        cmd.env_remove(key);
    }
    for (k, v) in extra_env {
        cmd.env(k, v);
    }

    let output = cmd
        .output()
        .await
        .with_context(|| format!("spawn {:?}", program.as_ref()))?;

    Ok(CmdOutput {
        status: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

pub async fn run_git(args: &[&str], cwd: Option<&Path>, extra_env: &HashMap<String, String>) -> Result<CmdOutput> {
    run("git", args, cwd, extra_env, &[]).await
}

/// Env keys stripped from every tmux client spawn (panes must not inherit these).
pub const TMUX_SCRUB_ENV: &[&str] = &["GITHUB_TOKEN"];

pub async fn run_tmux(args: &[&str]) -> Result<CmdOutput> {
    run("tmux", args, None, &HashMap::new(), TMUX_SCRUB_ENV).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_clears_requested_env_keys() {
        // `/usr/bin/env` prints the child environment; cleared keys must be absent.
        let out = run(
            "env",
            &[],
            None,
            &HashMap::from([("ANCHOR_TEST_KEEP".into(), "1".into())]),
            &["GITHUB_TOKEN"],
        )
        .await
        .unwrap();
        assert!(out.success());
        assert!(
            !out.stdout.lines().any(|l| l.starts_with("GITHUB_TOKEN=")),
            "GITHUB_TOKEN must not appear in child env"
        );
        assert!(out.stdout.lines().any(|l| l == "ANCHOR_TEST_KEEP=1"));
    }

    #[test]
    fn ensure_success_redacts_pat_in_stderr() {
        let out = CmdOutput {
            status: 1,
            stdout: String::new(),
            stderr: "fatal: Authorization: Bearer ghp_ShellLeakToken42 denied".into(),
        };
        let err = out.ensure_success("git fetch").unwrap_err();
        let msg = err.to_string();
        assert!(!msg.contains("ShellLeakToken42"), "leaked: {msg}");
        assert!(msg.contains("[redacted]"));
    }
}
