# Anchor

Rust service that syncs GitHub repos into git worktrees and orchestrates tmux panes for coding agents on a VPS.

**Hard Life Tech** · Core is open source (Apache-2.0) · Management SaaS is private

## Quick start

```bash
cp .env.example .env
# set GITHUB_TOKEN and GITHUB_USER

cargo run
curl -s localhost:8080/healthz   # OK
```

Or with Docker:

```bash
docker compose up --build -d
```

## Docs

- [PROJECT.md](PROJECT.md) — product source of truth
- [CONTRIBUTING.md](CONTRIBUTING.md) — setup and PR norms
- [docs/](docs/README.md) — features, ADRs, API contract, deployment

## Starting agents

1. `docker compose up --build -d` (or `cargo run` with CLIs on `PATH`)
2. Sign in → create/sync a project
3. Open **Terminal** on the project (or attach tmux session `agents`)
4. Complete Cursor / OpenCode auth in each pane on first use

Defaults: left pane `agent` (Cursor CLI), right pane `opencode`. Override via Settings or `CURSOR_CMD` / `OPENCODE_CMD`.

## Security

`GITHUB_TOKEN` stays in the Anchor **process** environment only. Never export it into agent tmux panes.
