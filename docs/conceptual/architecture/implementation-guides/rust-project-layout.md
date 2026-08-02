# Guide — Rust project layout

Dockerfile expects:

```text
cargo build --release
→ /app/target/release/anchor
COPY to /usr/local/bin/anchor
```

Suggested layout (v1, single binary):

```text
Cargo.toml          # package name = anchor, binary name = anchor
src/
  main.rs           # tracing init, bind, serve
  config.rs         # env → Config
  error.rs          # thiserror / anyhow boundaries
  github.rs         # reqwest list repos + cache
  git.rs            # tokio::process git helpers
  tmux.rs           # tokio::process tmux helpers
  api.rs            # axum routes
  templates/        # askama
```

Crates: `axum`, `tokio`, `serde`, `serde_json`, `reqwest`, `tracing`, `tracing-subscriber`, `anyhow`, `thiserror`, `askama` (+ axum integration as chosen).

No `git2` for worktrees in v1 ([ADR-0007](../../adr/ADR-0007-shell-out-git-tmux.md)).
