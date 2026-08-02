# F-003 — tmux agent orchestration

**Phase:** MVP

## Intent

Ensure a shared tmux session has one window per **project slug** with Cursor CLI and OpenCode in separate panes, each cwd'd to the agent **workspace root** (parent of sibling member worktrees).

## Rules

- Session name: `TMUX_SESSION` (default `agents`).
- Window name: **project slug**.
- Pane 0: `cd $PROJECTS_DIR/<slug>/cursor && $CURSOR_CMD`
- Pane 1: `cd $PROJECTS_DIR/<slug>/opencode && $OPENCODE_CMD`
- Check existence before create — **never** kill a live pane on re-sync.
- On legacy migration, rename window from short repo name → slug when ensuring (idempotent).
- `send-keys` only for *initial* launch.
- Prompting is operator attach (SSH / in-browser terminal), not a separate chat API.

## Acceptance

- [ ] Missing session/window/panes are created
- [ ] Existing live panes are left alone
- [ ] Agents do not inherit `GITHUB_TOKEN` (F-006)
