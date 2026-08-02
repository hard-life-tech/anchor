# Data storage — SQLite settings + live sources

Anchor persists **operator settings** in SQLite. Git/tmux remain the source of truth for repos and live agents.

## Sources of truth

| Concern | Source |
|---------|--------|
| Repo history | `$PROJECTS_DIR/<repo>/.bare` |
| Agent working trees | `cursor/`, `opencode/` worktrees |
| Live agent processes | tmux session/windows/panes |
| GitHub repo list cache | Optional **in-memory** only (TTL) |
| Agent CLI auth | Files under `/home/agent` (Cursor/OpenCode dirs) |
| Operator settings (cmds, args, notes) | SQLite (`DATABASE_URL` / `ANCHOR_DB`) |

## Implications

- Restart loses in-memory sync outcomes and live tmux; disk + settings DB remain.
- `GET /api/projects` still queries git/tmux/fs — not a projects table.
- See [ADR-0008](../../adr/ADR-0008-no-database.md).
