# Anchor Core Scaffold Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scaffold the Rust `anchor` binary so `GET /healthz` works locally and the Docker image build path matches the existing Dockerfile.

**Architecture:** Single axum/tokio binary with modules for config, API, git, tmux, and GitHub client. No database. Shell out to `git`/`tmux` when those modules land; Task 1–2 only bring up HTTP + config.

**Tech Stack:** Rust 2021, axum, tokio, serde, tracing, anyhow/thiserror; later reqwest, askama.

---

### Task 1: Cargo project matching Dockerfile

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `.gitignore`

- [ ] **Step 1: Write Cargo.toml**

```toml
[package]
name = "anchor"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "anchor"
path = "src/main.rs"

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
thiserror = "2"
```

- [ ] **Step 2: Minimal main with healthz**

```rust
use axum::{routing::get, Router};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let app = Router::new().route("/healthz", get(|| async { "OK" }));
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    tracing::info!("listening on {addr}");
    axum::serve(listener, app).await.expect("serve");
}
```

- [ ] **Step 3: Add .gitignore**

```gitignore
/target
.env
*.pem
```

- [ ] **Step 4: Run locally**

Run: `cargo run`
Expected: log `listening on 0.0.0.0:8080`
Run: `curl -s localhost:8080/healthz`
Expected: `OK`

- [ ] **Step 5: Commit** (only when user asks)

```bash
git add Cargo.toml src/main.rs .gitignore
git commit -m "feat: scaffold anchor binary with healthz"
```

---

### Task 2: Config from environment

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write failing unit test for missing token**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_requires_token() {
        let err = Config::from_env_map([("GITHUB_USER", "x")].into()).unwrap_err();
        assert!(err.to_string().contains("GITHUB_TOKEN"));
    }
}
```

(Adapt `from_env_map` helper for testability; production uses `std::env`.)

- [ ] **Step 2: Implement Config**

Fields: `github_token`, `github_user`, `projects_dir`, `tmux_session`, `cursor_cmd`, `opencode_cmd`, `port`, `log_level` — defaults per `PROJECT.md` §6. Never log `github_token`.

- [ ] **Step 3: Fail fast in main if config invalid**

- [ ] **Step 4: `cargo test` passes**

---

### Task 3: API router skeleton

**Files:**
- Create: `src/api.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Mount routes** `/api/repos`, `/api/projects`, `/api/projects/{repo}/sync`, `/api/projects/{repo}/status` returning `501` stubs with JSON `{ "error": "not implemented", "code": "NOT_IMPLEMENTED" }`.

- [ ] **Step 2: Keep `/healthz` working**

- [ ] **Step 3: Commit when asked**

---

### Task 4+: Git, tmux, GitHub modules

Follow [implementation guides](../../conceptual/architecture/implementation-guides/@index.md) and features F-001–F-006. Prefer TDD with tempdirs for git and scripted tmux where available.

---

## Plan self-review

- Covers Dockerfile binary name `anchor`.
- Does not invent a database.
- Token never logged in Task 2 notes.
- Further tasks deferred to guides to keep this plan executable for scaffold first.
