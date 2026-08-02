# Data storage — no database

v1 Anchor has **no SQL/embedded DB**.

## Sources of truth

| Concern | Source |
|---------|--------|
| Repo history | `$PROJECTS_DIR/<repo>/.bare` |
| Agent working trees | `cursor/`, `opencode/` worktrees |
| Live agent processes | tmux session/windows/panes |
| GitHub repo list cache | Optional **in-memory** only (TTL) |
| Agent CLI auth | Files under `/home/agent` (Cursor/OpenCode dirs) |

## Implications

- Restart loses in-memory cache and live tmux; disk state remains.
- `GET /api/projects` must query git/tmux/fs — not a table.
- `last_synced` if shown can be derived (e.g. fetch reflog / mtime) or omitted — do not invent a SQLite dependency for v1.

See [ADR-0008](../../adr/ADR-0008-no-database.md).
