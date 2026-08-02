# Contributing to Anchor

Thanks for helping with **Anchor** (Hard Life Tech). Core is Apache-2.0; the private Management SaaS lives elsewhere — do not add multi-tenant SaaS features here.

## Quick setup

```bash
cp .env.example .env
# set GITHUB_TOKEN (fine-grained PAT, repo scope) and GITHUB_USER

cargo test
cargo run
curl -s localhost:8080/healthz   # OK
```

Docker:

```bash
docker compose up --build -d
```

Never commit `.env`. Never log or export `GITHUB_TOKEN` into agent tmux panes.

## Development norms

- Rust 2021, `axum`, shell-outs to `git` / `tmux` (no `git2` for worktrees in v1)
- Dashboard: Askama + htmx (no SPA / frontend build)
- Prefer small, focused PRs with conventional commits (`feat:`, `fix:`, `docs:`, …)
- Keep `GITHUB_TOKEN` in the Anchor **process** environment only
- Do not force-push to `main`

## Tests

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

## Docs

Product source of truth: [`PROJECT.md`](PROJECT.md). Specs under [`docs/`](docs/README.md). Update docs when behavior changes.

## Security

- Fine-grained PAT with `repo` scope only
- No secrets in issues, PRs, logs, or API responses
- Service intended for private networks (e.g. Tailscale), not public internet in v1

## License

By contributing, you agree your contributions are licensed under Apache-2.0.
