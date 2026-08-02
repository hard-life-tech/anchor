# F-003 — tmux agent orchestration

**Phase:** MVP

## Intent

Ensure a shared tmux session has one window per repo with Cursor CLI and OpenCode running in separate panes, each cwd'd to its worktree.

## Rules

- Session name: `TMUX_SESSION` (default `agents`).
- Window name: repo short name.
- Pane 1: `cd cursor/ && $CURSOR_CMD`
- Pane 2: `cd opencode/ && $OPENCODE_CMD`
- Check existence before create — **never** kill a live pane on re-sync.
- `send-keys` only for *initial* launch.
- Prompting is operator attach (SSH/ttyd), not HTTP.

## Acceptance

- [ ] Missing session/window/panes are created
- [ ] Existing live panes are left alone
- [ ] Agents do not inherit `GITHUB_TOKEN` (F-006)
