# Deployment guide

Anchor v1 is a **single Docker Compose service** for one operator. Keep it off the public internet — Tailscale (or equivalent) only.

## Delivery modes

| Mode | Who runs it | What ships |
|------|-------------|------------|
| **Self-hosted** | Operator | OSS Core image + Compose |
| **Cloud (Hard Life Tech)** | Company | Same Core, plus private Management SaaS (future) |

Cloud is distribution and tenancy around Core — not a different sync engine. See [ADR-0006](conceptual/adr/ADR-0006-self-host-vs-cloud.md).

---

## Prerequisites

- Docker + Compose
- GitHub fine-grained PAT with **`repo` scope only**
- (Prod) Tailscale on the host or mesh access to the container network
- Optional: Coolify as Compose orchestrator behind Traefik

---

## Environment

Copy `.env.example` → `.env` (never commit `.env`):

| Variable | Required | Default | Notes |
|----------|----------|---------|-------|
| `GITHUB_TOKEN` | yes | — | PAT; Anchor **process** env only |
| `GITHUB_USER` | yes | — | Account whose repos to list |
| `PROJECTS_DIR` | no | `$HOME/projects` | Inside container: `/home/agent/projects` |
| `TMUX_SESSION` | no | `agents` | Shared tmux session name |
| `CURSOR_CMD` | no | `cursor-agent` | Left pane command |
| `OPENCODE_CMD` | no | `opencode` | Right pane command |
| `PORT` | no | `8080` | HTTP listen |
| `LOG_LEVEL` | no | `info` | tracing filter |

### Secret isolation (mandatory)

- `GITHUB_TOKEN` must live only in the Anchor service environment (`env_file` / orchestrator secrets).
- **Never** export `GITHUB_TOKEN` into agent tmux panes (Cursor / OpenCode).
- Agents authenticate themselves via their own OAuth/device flows; state persists under `/home/agent`.
- Coding agents working *on this repo* must not read `*.env` (see root `.cursorignore` / `opencode.json`).

---

## Local / VPS Compose

```bash
cp .env.example .env
# edit GITHUB_TOKEN and GITHUB_USER

docker compose up --build -d
curl -sS http://127.0.0.1:8080/healthz
# Compose healthcheck also probes /healthz inside the container
```

Volumes (from `docker-compose.yml`):

- `agent-home` → `/home/agent` (projects, agent auth, gitconfig)
- `tmux-sockets` → `/tmp` (shared with optional ttyd)

The container runs as non-root user **`agent`** (uid/gid 1000 by default).

### Container restart behavior

Restart kills the live tmux session (same as host reboot). Worktrees and agent auth on the volume remain. Next `POST .../sync` recreates the window/panes. This is a deliberate v1 tradeoff.

---

## Coolify / Traefik

1. Create a Docker Compose resource pointing at this repo.
2. Inject `GITHUB_TOKEN` / `GITHUB_USER` as secrets — not build args.
3. Remove host `ports:` publish; attach to the proxy network with internal labels.
4. Restrict access to Tailscale (or Coolify's private networking). Do not put Anchor on a public hostname without auth (auth is out of scope for v1).

---

## Optional ttyd sidecar

Uncomment the `ttyd` service in `docker-compose.yml` to attach a browser terminal to session `agents`. Run as the same uid (`1000:1000`) so it shares `/tmp/tmux-1000`.

Anchor's HTTP API does **not** relay prompts — attach via SSH or ttyd.

---

## First-time agent auth

After the first successful sync for a repo:

1. Attach to tmux (`tmux attach -t agents` or ttyd).
2. Complete Cursor CLI and OpenCode login in their panes once.
3. Auth persists on the `agent-home` volume across restarts.

---

## Cloud (future)

Hard Life Tech cloud will run Core images in a managed fleet with a private control plane (tenancy, billing, policy). Operators of OSS self-host builds are unaffected. Boundary: [ADR-0005](conceptual/adr/ADR-0005-oss-saas-split.md).

---

## Security checklist

- [ ] `.env` not in git; `.env` in `.gitignore` / `.cursorignore`
- [ ] No public ingress without Tailscale (or future auth)
- [ ] Process runs as `agent`, not root
- [ ] PAT is fine-grained, `repo` only
- [ ] Token never logged or returned by API
- [ ] Token never in agent pane environments
- [ ] Dockerfile agent CLI installers re-verified before prod rebuilds
