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

List Anchor projects (SQLite + disk) with member counts, worktree rollup, and tmux status.

**Response `200`:**

```json
{
  "projects": [
    {
      "slug": "platform",
      "name": "Platform",
      "member_count": 2,
      "members": [
        {
          "owner": "hard-life-tech",
          "name": "anchor",
          "full_name": "hard-life-tech/anchor",
          "private": false,
          "default_branch": "main",
          "worktrees": [
            {
              "agent": "cursor",
              "branch": "agent/cursor",
              "ahead": 0,
              "behind": 0,
              "dirty": false,
              "diverged": false
            }
          ]
        }
      ],
      "on_disk": true,
      "tmux_window_exists": true,
      "last_synced": "2026-08-02T10:05:00Z",
      "last_sync": {
        "outcome": "ok",
        "message": "synced",
        "at": "2026-08-02T10:05:00Z",
        "skipped_dirty": 0,
        "skipped_diverged": 0
      }
    }
  ]
}
```

`last_synced` / `last_sync` come from in-memory sync outcomes in the current process (lost on restart). Project membership persists in SQLite and `.anchor/project.json` — see [ADR-0008](conceptual/adr/ADR-0008-no-database.md) and [ADR-0010](conceptual/adr/ADR-0010-multi-repo-projects.md).

---

## `POST /api/projects`

Create a project.

**Body:**

```json
{
  "name": "Platform",
  "slug": "platform",
  "repos": ["hard-life-tech/anchor", "hard-life-tech/docs"]
}
```

`slug` is optional — derived from `name` when omitted (lowercase, hyphenated). `repos` entries are `owner/name` (preferred) or a unique short name resolved via GitHub list.

**Response `201`:** project status object (same shape as one element of `GET /api/projects`).

**Errors:** `400` invalid slug/body; `409` slug already exists; `404` unknown GitHub repo.

---

## `GET /api/projects/{slug}`

Detailed status for one project.

**Response `200`:** same shape as one element of `projects` in `GET /api/projects`.

**Errors:** `404` if unknown.

---

## `PATCH /api/projects/{slug}`

Update display `name` (and optionally `default_branch` hint).

**Body:** `{ "name": "…", "default_branch": "main" }` (fields optional).

**Response `200`:** updated project status.

---

## `DELETE /api/projects/{slug}`

Remove project metadata from SQLite and `.anchor/project.json`. Does **not** delete git data on disk (operator must clean up manually). Does not kill tmux panes.

**Response `204`.**

---

## `POST /api/projects/{slug}/repos`

Add member repos.

**Body:** `{ "repos": ["owner/name", …] }`

**Response `200`:** updated project status. Sync is **not** implied — call sync after.

---

## `DELETE /api/projects/{slug}/repos/{owner}/{repo}`

Remove a member from metadata. If worktrees are dirty, skip disk deletion and return `409` with reason; clean members may have bare/worktrees removed when safe.

**Response `200`:** updated project status, or `409` when dirty trees block removal.

---

## `POST /api/projects/{slug}/sync`

Idempotent sync of **all** members + ensure tmux window named `{slug}`.

**Behavior:**

1. For each member: if `$PROJECTS_DIR/{slug}/.bares/<owner>__<repo>` missing → bare clone; create `cursor/` and `opencode/` worktrees under `<owner>__<repo>/`.
2. If present → `git fetch`; per worktree fast-forward only if clean and not diverged.
3. Ensure tmux session/window/panes with cwd = workspace roots (`…/{slug}/cursor`, `…/opencode`). Never restart a live pane.

**Response `200`:**

```json
{
  "slug": "platform",
  "repos": [
    {
      "full_name": "hard-life-tech/anchor",
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
      ]
    }
  ],
  "tmux": {
    "session": "agents",
    "window": "platform",
    "created_window": false,
    "panes_ensured": true
  }
}
```

Suggested `action` values: `created` | `fast_forwarded` | `already_up_to_date` | `skipped_dirty` | `skipped_diverged`.

---

## `POST /api/projects/{slug}/repos/{owner}/{repo}/sync`

Sync a **single** member into an existing project (incremental add path after `POST …/repos`).

**Response `200`:** same per-repo object as one element of `repos` in the full sync response, plus `tmux` ensure.

---

## Legacy: `POST /api/projects/{repo}/sync`

Thin wrap: treat `{repo}` as a short GitHub name, ensure a single-member project (slug = short name when unique), then sync. Prefer the project-scoped routes above for multi-repo work.

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
| `GET` | `/projects/{slug}` | Project detail (members, sync, terminal links) |
| `GET` | `/projects/{slug}/terminal` | xterm.js page (`?agent=cursor\|opencode`) |
| `WS` | `/ws/terminal/{slug}/{agent}` | PTY attach to tmux pane (auth via cookie) |
| `GET` | `/partials/projects` | Project rows (member count + sync rollup) |
| `GET` | `/partials/repos` | GitHub repo list with “Add to project…” |
| `POST` | `/partials/projects` | Create project (form) |
| `POST` | `/partials/projects/{slug}/sync` | Sync all members + OOB flash |

`GET /` paints immediately. htmx loads `/partials/*` on `hx-trigger="load"`. Dashboard is not a SPA. See [design-system.md](design-system.md).

---

## Non-goals for the API (v1)

- Prompt relay into agent panes (use the browser terminal / agent TUI)
- Multi-user accounts / SSO
- Webhooks
- Streaming agent logs outside the PTY WebSocket
