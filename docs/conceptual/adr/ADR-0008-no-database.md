# ADR-0008 — Settings database (SQLite)

**Status:** Superseded prior “no database” stance for **settings only**  
**Date:** 2026-08-02  
**Updated:** 2026-08-02

## Context

Orchestration truth still lives in git and tmux. Operators also need durable prefs: Cursor/OpenCode commands, args, enable flags, notes. Env-only overrides require container restarts and cannot be edited from the phone UI.

## Decision

- **SQLite** (rusqlite, bundled) stores a `settings` key/value table.
- Default path: `$HOME/projects/anchor.db` (volume-backed under `/home/agent` in Compose).
- Override via `DATABASE_URL=sqlite:/path` or `ANCHOR_DB`.
- Do **not** mirror project/worktree/tmux state into SQL — continue querying disk/git/tmux on demand.
- `last_synced` remains in-process memory unless a later ADR expands persistence.

## Consequences

- Settings survive restarts; sync uses DB overrides over `CURSOR_CMD` / `OPENCODE_CMD` env defaults.
- Ops: back up `anchor.db` with the agent volume.
- Prior “no database in v1” guidance applied to orchestration state — settings are an explicit exception.
