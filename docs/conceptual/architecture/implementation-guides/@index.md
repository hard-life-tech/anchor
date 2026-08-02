# Implementation guides

Practical guides for Phase 1 scaffold. Prefer TDD; shell out to real `git`/`tmux` in integration tests where possible (or script fixtures).

| Guide | Module |
|-------|--------|
| [rust-project-layout.md](rust-project-layout.md) | Crate layout matching Dockerfile |
| [git-worktrees.md](git-worktrees.md) | Bare clone + worktree sync |
| [tmux-orchestration.md](tmux-orchestration.md) | Idempotent session ensure |
| [config-and-secrets.md](config-and-secrets.md) | Env loading + token hygiene |

Full bite-sized plan: [../../superpowers/plans/2026-08-02-anchor-core-scaffold.md](../../superpowers/plans/2026-08-02-anchor-core-scaffold.md).
