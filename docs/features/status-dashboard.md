# F-004 — Project status API + dashboard

**Phase:** MVP  
**API:** `GET /api/projects`, `GET /api/projects/{repo}/status`  
**UI:** `GET /`

## Intent

Let the operator see on-disk / git / tmux state from a phone browser and trigger sync.

## Behavior

- Derive status by querying filesystem + `git` + `tmux` (no DB).
- Dashboard: Askama + htmx; list projects and sync actions.
- After container restart, disk status still correct even if tmux is gone.

## Acceptance

- [ ] API matches [api-contract.md](../api-contract.md)
- [ ] Dashboard usable on narrow viewports
- [ ] Status correct with tmux absent
