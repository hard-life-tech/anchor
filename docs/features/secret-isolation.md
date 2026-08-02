# F-006 — Secret / PAT isolation

**Phase:** MVP (security requirement)

## Intent

Keep GitHub credentials in the Anchor **process** environment only. Agent CLIs must not see `GITHUB_TOKEN`. Coding agents editing this repo must not read `.env` files.

## Rules

1. Load `GITHUB_TOKEN` in Anchor only; pass to `reqwest` / git HTTPS as needed via process-local env or credential helper scoped to Anchor's git invocations — **not** via `tmux setenv` into panes.
2. When spawning panes, clear or omit `GITHUB_TOKEN` from the pane environment.
3. Never log or return the token.
4. Repo agent configs deny reading `*.env` (see root `AGENTS.md` / `opencode.json` / `.cursor`).
5. Document: never export `GITHUB_TOKEN` into agent tmux panes manually.

## Acceptance

- [ ] Documented in deployment guide
- [ ] Pane env inspection (manual or test) shows no token
- [ ] API responses never contain token substrings
