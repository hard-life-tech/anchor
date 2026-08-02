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
- [docs/](docs/README.md) — features, ADRs, API contract, deployment

## Security

`GITHUB_TOKEN` stays in the Anchor **process** environment only. Never export it into agent tmux panes.
