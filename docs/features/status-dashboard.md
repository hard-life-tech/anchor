# F-004 — Project status API + dashboard

**Phase:** MVP  
**API:** `GET /api/projects`, `GET /api/projects/{slug}`  
**UI:** `GET /`, `GET /projects/{slug}`

## Intent

Let the operator see project membership, on-disk / git / tmux state from a phone browser, create multi-repo projects, and trigger sync.

## Behavior

- Membership from SQLite + `.anchor/project.json`; live status from filesystem + `git` + `tmux`.
- Home table: **projects** (member count, sync rollup), not flat repo=project rows.
- Project create: name + multi-select GitHub repos.
- Project detail: member list, per-repo sync, Open terminal at project level.
- Repos list: “Add to project…” in addition to legacy solo sync wrap.
- Dashboard: Askama + htmx; `GET /` returns shell + skeletons; lists load via `/partials/*` after paint.
- GitHub `/user/repos` uses a short in-memory TTL cache; Sync invalidates it.
- After container restart, membership + disk status still correct even if tmux is gone.

## Acceptance

- [x] API matches [api-contract.md](../api-contract.md)
- [x] Dashboard usable on narrow viewports (Askama + htmx)
- [x] Status correct with tmux absent
