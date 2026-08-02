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
            Err(anyhow!(
                "{label} failed (exit {}): {}",
                self.status,
                self.stderr.trim()
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

pub async fn run_tmux(args: &[&str]) -> Result<CmdOutput> {
    run("tmux", args, None, &HashMap::new(), &["GITHUB_TOKEN"]).await
}
