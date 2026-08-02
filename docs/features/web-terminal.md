# F-013 — Built-in web terminal

xterm.js in the dashboard attaches to existing tmux panes for Cursor / OpenCode.

## Behavior

- `GET /projects/{repo}/terminal?agent=cursor|opencode` — terminal page (auth required).
- `WS /ws/terminal/{repo}/{agent}` — bidirectional PTY to `tmux attach-session` after zooming the target pane (`0` = cursor, `1` = opencode).
- If the project or tmux window is missing, the page shows a CTA to sync.
- `GITHUB_TOKEN` is stripped from the PTY child environment.

## Non-goals

- Separate “chat panel” product — the agent TUI *is* the chat surface.
- Multiplex both panes in one xterm without zoom (tabs switch agents via zoom + re-attach).
