# ADR-0008 — SQLite for settings (and project metadata)

**Status:** Accepted (evolved)  
**Date:** 2026-08-02  
**Updated:** 2026-08-02

## Context

Orchestration truth for git history and live agents still lives in disk/git/tmux. Operators need durable prefs (Cursor/OpenCode commands). Multi-repo projects ([ADR-0010](ADR-0010-multi-repo-projects.md)) also need durable membership lists that survive restarts and can be edited from the API/UI.

## Decision

- **SQLite** (rusqlite, bundled) stores:
  - `settings` key/value (agent commands, prefs)
  - `projects` / `project_repos` (slug, name, member repos) — mirrored to `.anchor/project.json` on disk
- Default path: `$HOME/projects/anchor.db` (volume-backed under `/home/agent` in Compose).
- Override via `DATABASE_URL=sqlite:/path` or `ANCHOR_DB`.
- Do **not** mirror live worktree dirtiness or tmux pane PIDs into SQL — continue querying disk/git/tmux on demand.
- `last_synced` remains in-process memory unless a later ADR expands persistence.

## Consequences

- Settings and project membership survive restarts; sync uses DB overrides over `CURSOR_CMD` / `OPENCODE_CMD` env defaults.
- Ops: back up `anchor.db` with the agent volume (and project trees under `PROJECTS_DIR`).
- Prior “no database in v1” applied only to ephemeral orchestration state — settings and project metadata are explicit exceptions.
