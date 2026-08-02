# Data storage — SQLite + live sources

Anchor persists **operator settings** and **project membership** in SQLite. Git/tmux remain the source of truth for history and live agents. Disk also mirrors project metadata at `.anchor/project.json`.

## Sources of truth

| Concern | Source |
|---------|--------|
| Project membership | SQLite `projects` / `project_repos` + `.anchor/project.json` |
| Repo history | `$PROJECTS_DIR/<slug>/.bares/<owner>__<repo>/` |
| Agent working trees | `cursor/<owner>__<repo>/`, `opencode/<owner>__<repo>/` |
| Live agent processes | tmux session/windows/panes (window = project slug) |
| GitHub repo list cache | Optional **in-memory** only (TTL) |
| Agent CLI auth | Files under `/home/agent` (Cursor/OpenCode dirs) |
| Operator settings (cmds, args, notes) | SQLite `settings` (`DATABASE_URL` / `ANCHOR_DB`) |

## Implications

- Restart loses in-memory sync outcomes and live tmux; disk + settings/project DB remain.
- `GET /api/projects` joins SQLite membership with git/tmux/fs status.
- See [ADR-0008](../../adr/ADR-0008-no-database.md) and [ADR-0010](../../adr/ADR-0010-multi-repo-projects.md).
