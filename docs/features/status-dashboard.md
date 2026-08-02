# F-004 — Project status API + dashboard

**Phase:** MVP  
**API:** `GET /api/projects`, `GET /api/projects/{slug}`  
**UI:** `GET /`, `GET /projects/{slug}`

## Intent

Let the operator see project membership, on-disk / git / tmux state from a phone browser, create multi-repo projects, and trigger sync.

## Behavior

- Membership from SQLite + `.anchor/project.json`.
- **List inventory is cheap:** DB + filesystem presence + one tmux `list-windows` (15s in-memory cache). No GitHub and no per-row `git status` on `/partials/projects` or `GET /api/projects`. Visibility badges use stored `project_repos.private`.
- **Deep status** (dirty / ahead / behind) on `GET /api/projects/{slug}` and the project detail page.
- Home table: **projects** (member count, sync rollup), not flat repo=project rows.
- Project create: name + multi-select GitHub repos (create picker uses warm cache only — never blocks `/` on GitHub).
- Project detail: member list, per-repo sync, Open terminal at project level.
- Repos list: paginated first paint (40) + “Show more”; 180s GitHub `/user/repos` cache (sync does **not** invalidate it).
- Dashboard: Askama + htmx; `GET /` returns shell + skeletons; projects load on `load`, repos on `load delay:50ms`.
- After container restart, membership + disk status still correct even if tmux is gone.

## Acceptance

- [x] API matches [api-contract.md](../api-contract.md)
- [x] Dashboard usable on narrow viewports (Askama + htmx)
- [x] Status correct with tmux absent
