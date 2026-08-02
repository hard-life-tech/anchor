# F-004 — Project status API + dashboard

**Phase:** MVP  
**API:** `GET /api/projects`, `GET /api/projects/{repo}/status`  
**UI:** `GET /`

## Intent

Let the operator see on-disk / git / tmux state from a phone browser and trigger sync.

## Behavior

- Derive status by querying filesystem + `git` + `tmux` (no DB).
- Dashboard: Askama + htmx; `GET /` returns shell + skeletons; lists load via `/partials/*` after paint.
- GitHub `/user/repos` uses a short in-memory TTL cache; Sync invalidates it.
- After container restart, disk status still correct even if tmux is gone.

## Acceptance

- [x] API matches [api-contract.md](../api-contract.md)
- [x] Dashboard usable on narrow viewports (Askama + htmx)
- [x] Status correct with tmux absent
