# Architecture

## System overview

```
GitHub ──clone/fetch──► Anchor (axum/tokio)
                           │
                           ▼
              /home/agent/projects/<repo>/
                    .bare/   cursor/   opencode/
                           │
                           ▼
              tmux session "agents"
              window <repo>: [cursor-agent | opencode]
```

Anchor **only** orchestrates git + tmux + a small HTTP/dashboard surface. It does not relay prompts and does not persist a database.

## Key decisions

Documented as ADRs under [adr/](../adr/@index.md). Highlights:

- Rust + axum/tokio ([ADR-0001](../adr/ADR-0001-rust-stack.md))
- Per-agent git worktrees ([ADR-0002](../adr/ADR-0002-worktree-strategy.md))
- tmux isolation ([ADR-0003](../adr/ADR-0003-agent-isolation.md))
- PAT / env isolation ([ADR-0004](../adr/ADR-0004-token-handling.md))
- OSS Core vs private SaaS ([ADR-0005](../adr/ADR-0005-oss-saas-split.md))
- Self-host + cloud delivery ([ADR-0006](../adr/ADR-0006-self-host-vs-cloud.md))
- Shell out to `git`/`tmux` ([ADR-0007](../adr/ADR-0007-shell-out-git-tmux.md))
- No database ([ADR-0008](../adr/ADR-0008-no-database.md))

## Subsections

| Path | Topic |
|------|-------|
| [db/](db/@index.md) | Why no DB; sources of truth |
| [diagrams/](diagrams/@index.md) | Flows and sequences |
| [implementation-guides/](implementation-guides/@index.md) | How to implement modules |

## Runtime layout

See `PROJECT.md` §5. Container user: non-root `agent`. Volume: `/home/agent`.
