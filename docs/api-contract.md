# HTTP API contract (v1)

Base URL: `http://<host>:$PORT` (default `8080`).  
Auth: **session cookie** required on all routes except `GET /healthz`, `GET|POST /login`, and `/static/*`. Set `ANCHOR_PASSWORD` (and optional `ANCHOR_USER`). Do not expose publicly without Tailscale (or equivalent) even with a password.

Unauthenticated API/WebSocket calls return `401`:

```json
{ "error": "authentication required", "code": "UNAUTHORIZED" }
```

All other JSON uses `Content-Type: application/json`. Errors return:

```json
{ "error": "human-readable message", "code": "MACHINE_CODE" }
```

Never include secrets (`GITHUB_TOKEN`, `ANCHOR_PASSWORD`, agent OAuth tokens) in responses or error details.

---

## `GET /healthz`

Liveness for orchestrators.

**Response:** `200` plain text `OK` (or empty body). No JSON required.

---

## `GET /api/repos`

List repos visible to `GITHUB_TOKEN` (authenticated `GET /user/repos`: private, public, and org membership).

**Caching:** In-memory, short TTL (suggested 2–5 minutes) to respect GitHub rate limits.

**Enterprise:** Set `GITHUB_API_URL` / `GITHUB_HOST` for GHES (see `PROJECT.md`).

**Response `200`:**

```json
{
  "repos": [
    {
      "name": "anchor",
      "full_name": "hard-life-tech/anchor",
      "private": false,
      "default_branch": "main",
      "clone_url": "https://github.com/hard-life-tech/anchor.git"
    }
  ],
  "cached_at": "2026-08-02T10:00:00Z"
}
```

---

## `GET /api/projects`

List on-disk projects under `PROJECTS_DIR` with git + tmux status.

**Response `200`:**

```json
{
  "projects": [
    {
      "name": "anchor",
      "on_disk": true,
      "worktrees": [
        {
          "agent": "cursor",
          "branch": "agent/cursor",
          "ahead": 0,
          "behind": 0,
          "dirty": false,
          "diverged": false
        },
        {
          "agent": "opencode",
          "branch": "agent/opencode",
          "ahead": 1,
          "behind": 0,
          "dirty": true,
          "diverged": false
        }
      ],
      "tmux_window_exists": true,
      "last_synced": "2026-08-02T10:05:00Z",
      "last_sync": {
        "outcome": "ok",
        "message": "synced",
        "at": "2026-08-02T10:05:00Z",
        "skipped_dirty": 0,
        "skipped_diverged": 0
      },
      "visibility": "private"
    }
  ]
}
```

`last_synced` / `last_sync` come from in-memory sync outcomes in the current process (lost on restart). `visibility` is `public` / `private` when the repo is still visible via the GitHub list API.

Operator settings (agent commands) persist in SQLite — see `/api/settings` and [ADR-0008](conceptual/adr/ADR-0008-no-database.md).

---

## `POST /api/projects/{repo}/sync`

Idempotent sync. `{repo}` is the short repo name (directory name under `PROJECTS_DIR`).

**Behavior:**

1. If `$PROJECTS_DIR/{repo}/.bare` missing: bare clone, create `cursor/` and `opencode/` worktrees on `agent/cursor` and `agent/opencode` from `origin/<default_branch>`.
2. If present: `git fetch` in `.bare`. Per worktree, fast-forward from `origin/<default_branch>` only if clean and not diverged; otherwise leave untouched and flag.
3. Ensure tmux session/window/panes exist. Never restart a live pane.

**Response `200`:**

```json
{
  "name": "anchor",
  "created": false,
  "fetched": true,
  "worktrees": [
    {
      "agent": "cursor",
      "action": "fast_forwarded",
      "dirty": false,
      "diverged": false
    },
    {
      "agent": "opencode",
      "action": "skipped_dirty",
      "dirty": true,
      "diverged": false
    }
  ],
  "tmux": {
    "session": "agents",
    "window": "anchor",
    "created_window": false,
    "panes_ensured": true
  }
}
```

Suggested `action` values: `created` | `fast_forwarded` | `already_up_to_date` | `skipped_dirty` | `skipped_diverged`.

**Errors:** `404` unknown GitHub repo; `502` git/GitHub failure (auth failures are classified — e.g. missing/invalid token for private repos — without echoing secrets); `409` optional if policy later forbids sync — not required in v1.

---

## `GET /api/projects/{repo}/status`

Detailed status for one project.

**Response `200`:** same shape as one element of `projects` in `GET /api/projects`, plus optional raw fields for debugging (branch SHAs). Still no secrets.

**Errors:** `404` if not on disk.

---

## `GET|POST /api/settings`

Read or update operator settings (Cursor/OpenCode commands, args, enable flags, notes). Values override env defaults at sync time. Empty command strings mean “use env default”.

**Response `200`:** JSON including `cursor_cmd`, `opencode_cmd`, `cursor_args`, `opencode_args`, `cursor_enabled`, `opencode_enabled`, `install_notes`, `resolved`, `env_defaults`, `database`.

---

## Dashboard (non-API)

| Method | Path | Notes |
|--------|------|-------|
| `GET` | `/login` | Login form (public) |
| `POST` | `/login` | Sets session cookie |
| `POST` | `/logout` | Clears session |
| `GET` | `/` | Shell + skeletons only (Askama); auth required |
| `GET` | `/settings` | Agent config UI |
| `GET` | `/projects/{repo}/terminal` | xterm.js page (`?agent=cursor\|opencode`) |
| `WS` | `/ws/terminal/{repo}/{agent}` | PTY attach to tmux pane (auth via cookie) |
| `GET` | `/partials/projects` | Project rows (disk + git + tmux) |
| `GET` | `/partials/repos` | GitHub repo list (`/user/repos`, short TTL cache) |
| `POST` | `/partials/projects/{repo}/sync` | Sync + OOB flash; invalidates repo cache |

`GET /` paints immediately. htmx loads `/partials/*` on `hx-trigger="load"`. Dashboard is not a SPA. See [design-system.md](design-system.md).

---

## Non-goals for the API (v1)

- Prompt relay into agent panes (use the browser terminal / agent TUI)
- Multi-user accounts / SSO
- Webhooks
- Streaming agent logs outside the PTY WebSocket
