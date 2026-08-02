# Anchor

**Anchor** is a Rust service that clones and syncs GitHub repos onto a VPS and wires each repo to two coding-agent CLIs in tmux — so sessions stay persistently *anchored* to the host and can be driven from a phone.

| | |
|---|---|
| **Company** | Hard Life Tech |
| **Product** | Anchor (Core OSS; Management SaaS private) |
| **License** | Apache-2.0 (Core) |
| **Delivery** | Self-hosted and Hard Life Tech cloud |

Informal planning name **Forge** is retired for public branding; see [docs/glossary.md](docs/glossary.md).

---

## 1. Problem

Operators run multiple AI coding agents against many GitHub repos. Sessions die on laptop sleep, auth is scattered, and there is no single place to sync repos and attach agents. Anchor keeps worktrees and tmux panes on a VPS the operator controls.

## 2. Goals (v1)

- List GitHub repos for a configured user/token.
- Sync a repo to disk as a bare clone plus two agent worktrees.
- Ensure a tmux window with two panes (Cursor CLI + OpenCode) without killing live work.
- Expose a small HTTP API and a minimal server-rendered dashboard.
- Keep `GITHUB_TOKEN` in the Anchor process environment only.

## 3. Non-goals (v1)

- Multi-tenant SaaS / billing / SSO (private Management product).
- Prompt relay into agent panes.
- GitHub App / webhooks.
- Built-in web terminal (optional ttyd sidecar only).
- Dashboard login (Tailscale trust).
- Database.

## 4. Product boundary

```
Anchor Core (OSS)  →  git sync · worktrees · tmux · HTTP · local dashboard
        │
        ▼ optional
Anchor Management (private)  →  multi-tenant control plane, billing, fleets
```

Do not implement Management SaaS inside this repository.

## 5. Runtime layout

Container runs as non-root user `agent` (uid/gid 1000 by default). Primary volume: `/home/agent`.

```
$PROJECTS_DIR/<repo>/
  .bare/       # bare clone
  cursor/      # worktree on agent/cursor
  opencode/    # worktree on agent/opencode
```

Shared tmux session `$TMUX_SESSION` (default `agents`); one window per repo.

## 6. Environment

| Variable | Required | Default | Notes |
|----------|----------|---------|-------|
| `GITHUB_TOKEN` | yes | — | Fine-grained PAT, `repo` scope; **process env only** |
| `GITHUB_USER` | yes | — | Account whose repos to list |
| `PROJECTS_DIR` | no | `$HOME/projects` | Inside container: `/home/agent/projects` |
| `TMUX_SESSION` | no | `agents` | Shared tmux session name |
| `CURSOR_CMD` | no | `cursor-agent` | Left pane command |
| `OPENCODE_CMD` | no | `opencode` | Right pane command |
| `PORT` | no | `8080` | HTTP listen |
| `LOG_LEVEL` | no | `info` | tracing filter |

Never log or echo `GITHUB_TOKEN`. Never export it into agent tmux panes.

## 7. Stack

- Rust 2021, tokio, axum
- serde / serde_json, reqwest, tracing / tracing-subscriber
- anyhow / thiserror
- Askama + htmx for the dashboard (no SPA)
- Shell out to `git` and `tmux` (no `git2` for worktrees in v1)

## 8. HTTP surface

See [docs/api-contract.md](docs/api-contract.md).

- `GET /healthz`
- `GET /api/repos`
- `GET /api/projects`
- `POST /api/projects/{repo}/sync`
- `GET /api/projects/{repo}/status`
- `GET /` — Askama dashboard

## 9. Sync rules

1. Missing `.bare` → bare clone; create `cursor/` and `opencode/` worktrees from `origin/<default_branch>`.
2. Present → `git fetch`; per worktree, `--ff-only` merge from `origin/<default>` only if clean and not diverged; otherwise skip and report.
3. Ensure tmux session/window/panes. Never kill or restart a live pane.
4. All git/tmux ops idempotent. No force-push. No overwrite of dirty worktrees.

## 10. Security invariants

- `GITHUB_TOKEN` only in Anchor process env.
- Agents authenticate via their own OAuth/device flows.
- Coding agents editing *this* repo must not read `*.env`.
- Process runs as `agent`, not root.
- Keep the service off the public internet (Tailscale).

## 11. Deployment

Docker multi-stage build → binary `anchor`. Compose: [docker-compose.yml](docker-compose.yml). Guide: [docs/deployment-guide.md](docs/deployment-guide.md).

## 12. Documentation

Primary tree: [docs/README.md](docs/README.md). Timeline: [docs/project-timeline.md](docs/project-timeline.md).

## 13. Out of scope for Core OSS

GitHub App, webhooks, dashboard auth, built-in web terminal, multi-user Core, Management SaaS features.

## 14. MVP acceptance criteria

- [ ] Fresh `POST /api/projects/{repo}/sync` creates `.bare` + both worktrees + tmux window with panes.
- [ ] Re-sync is idempotent (no duplicate windows/worktrees, no force overwrite).
- [ ] Dirty or diverged worktrees are reported and left untouched.
- [x] `GET /api/projects` is accurate after container restart (tmux gone, disk intact) — inventory from disk; Compose e2e with live PAT still open.
- [ ] `GET /healthz` returns `OK`; Compose brings up with only `GITHUB_TOKEN` + `GITHUB_USER`.
- [x] Token never appears in API responses, logs, or agent pane environments — scrub + redaction unit-tested; live pane e2e open.
