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
| `GITHUB_TOKEN` | yes | — | PAT with private repo access; Anchor **process** env only |
| `GITHUB_USER` | yes | — | Account shown on the dashboard |
| `ANCHOR_PASSWORD` | yes | — | Dashboard / API login password |
| `ANCHOR_USER` | no | `admin` | Login username |
| `ANCHOR_SESSION_SECRET` | no | derived | Cookie HMAC key |
| `ANCHOR_COOKIE_SECURE` | no | `false` | `true` when served over HTTPS |
| `DATABASE_URL` | no | `sqlite:$HOME/projects/anchor.db` | Settings DB |
| `GITHUB_HOST` | no | `github.com` | GHES hostname if not github.com |
| `GITHUB_API_URL` | no | derived from host | Override REST base for GHES |
| `PROJECTS_DIR` | no | `$HOME/projects` | Inside container: `/home/agent/projects` |
| `TMUX_SESSION` | no | `agents` | Shared tmux session name |
| `CURSOR_CMD` | no | `agent` | Left pane Cursor CLI (`agent` / `cursor-agent`; Settings UI can override) |
| `OPENCODE_CMD` | no | `opencode` | Right pane command (Settings UI can override) |
| `PORT` | no | `8080` | HTTP listen |
| `LOG_LEVEL` | no | `info` | tracing filter |

### Auth (mandatory)

- Set a strong `ANCHOR_PASSWORD` before exposing port 8080 on any network.
- Prefer Tailscale (or equivalent) even with a password — do **not** put Anchor on a public hostname without both network isolation and auth.
- Sign in at `/login`; session cookie `anchor_session` gates the dashboard, API, settings, and terminal WebSocket.
- `/healthz` stays public for orchestrator probes.

### Secret isolation (mandatory)

- `GITHUB_TOKEN` must live only in the Anchor service environment (`env_file` / orchestrator secrets).
- **Never** export `GITHUB_TOKEN` into agent tmux panes (Cursor / OpenCode).
- Agents authenticate themselves via their own OAuth/device flows; state persists under `/home/agent`.
- Coding agents working *on this repo* must not read `*.env` (see root `.cursorignore` / `opencode.json`).
- Never commit `.env`, real passwords, or PATs.
---

## Local / VPS Compose

```bash
cp .env.example .env
# edit GITHUB_TOKEN, GITHUB_USER, ANCHOR_PASSWORD

docker compose up --build -d
curl -sS http://127.0.0.1:8080/healthz
# Open http://127.0.0.1:8080/login — then Projects → Terminal, or Settings
# Compose healthcheck also probes /healthz inside the container
```

Volumes (from `docker-compose.yml`):

- `agent-home` → `/home/agent` (projects, agent auth, gitconfig, **settings SQLite**)
- `tmux-sockets` → `/tmp` (shared with optional ttyd)

The container runs as non-root user **`agent`** (uid/gid 1000 by default).

### Browser terminals

After sync creates a tmux window, open **Terminal** on a project row (or `/projects/{repo}/terminal?agent=cursor`). Tabs switch Cursor / OpenCode panes. The agent TUI is the chat surface.

### Container restart behavior

Restart kills the live tmux session (same as host reboot). Worktrees, agent auth, and the settings DB on the volume remain. Next `POST .../sync` recreates the window/panes. This is a deliberate v1 tradeoff.
---

## Coolify / Traefik

1. Create a Docker Compose resource pointing at this repo.
2. Inject `GITHUB_TOKEN` / `GITHUB_USER` as secrets — not build args.
3. Remove host `ports:` publish; attach to the proxy network with internal labels.
4. Restrict access to Tailscale (or Coolify's private networking). Require `ANCHOR_PASSWORD`; do not put Anchor on a public hostname without both mesh isolation and auth.

Use the prod override (drops host port publish):

```bash
docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d --build
```

See [`docker-compose.prod.yml`](../docker-compose.prod.yml).

---

## Tailscale + Coolify checklist

Copy-paste operator checklist for a private VPS deploy.

### Host / Tailscale

- [ ] VPS has Docker + Compose installed
- [ ] Tailscale installed and logged in on the host (`tailscale status` shows the node)
- [ ] MagicDNS or known Tailscale IP recorded for later access
- [ ] Host firewall: do **not** open `8080/tcp` to the public internet
- [ ] Prefer accessing Anchor only via Tailscale IP / MagicDNS name

### Secrets

- [ ] Copy `.env.example` → `.env` on the host (or Coolify secret store)
- [ ] Set `GITHUB_TOKEN` (fine-grained PAT, `repo` only) and `GITHUB_USER`
- [ ] Confirm `.env` is not committed (`git check-ignore -v .env`)
- [ ] Never put the PAT in Dockerfile `ARG` / build-time env

### Compose (Tailscale-only, no public ports)

```bash
cp .env.example .env   # edit token + user
docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d --build
docker compose ps
docker compose exec anchor curl -fsS http://127.0.0.1:8080/healthz
```

- [ ] `healthz` returns `OK` from inside the container
- [ ] From a Tailscale peer: reach the service via proxy or `host:8080` only if you intentionally published ports (dev `docker-compose.yml` alone). Prod override publishes **no** host ports — reach via Coolify/Traefik on the Docker network, or temporarily use the base compose for Tailscale-host-local publish.

### Coolify

- [ ] New resource → Docker Compose → this repo (or uploaded compose)
- [ ] Add override file `docker-compose.prod.yml` (or remove `ports:` in the Coolify editor)
- [ ] Attach service to Coolify’s proxy network; set Traefik labels / Coolify domains as needed
- [ ] Domain should resolve only on Tailscale (split DNS / private hostname) — not a public A record without auth
- [ ] Inject `GITHUB_TOKEN` and `GITHUB_USER` as runtime secrets
- [ ] Deploy; confirm Coolify healthcheck or `GET /healthz` succeeds
- [ ] Confirm Logs UI never shows a raw PAT (deny-path redaction is built in; still avoid echoing secrets in custom commands)

### Post-deploy smoke

- [ ] `GET /healthz` → `OK`
- [ ] `GET /` loads the dashboard
- [ ] `GET /api/repos` lists repos (PAT works)
- [ ] One `POST /api/projects/{repo}/sync` creates worktrees + tmux window
- [ ] `tmux attach -t agents` (or ttyd) — panes have **no** `GITHUB_TOKEN` in env

### Rollback / restart notes

- Restart drops live tmux panes; disk worktrees and agent auth on `agent-home` remain
- Re-run sync to recreate windows after restart

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
