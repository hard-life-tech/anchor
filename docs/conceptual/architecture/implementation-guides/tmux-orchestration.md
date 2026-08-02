# Guide — tmux orchestration

## Ensure session

```bash
tmux has-session -t "$TMUX_SESSION" 2>/dev/null || tmux new-session -d -s "$TMUX_SESSION"
```

## Ensure window

```bash
tmux list-windows -t "$TMUX_SESSION" -F '#{window_name}'
# if repo name missing → new-window -t "$TMUX_SESSION" -n "$REPO"
```

## Launch agents (initial only)

Clear `GITHUB_TOKEN` in the environment inherited by `send-keys` / pane creation. Explicitly set `cwd` to worktree paths.

```bash
# illustrative — implement via split-window -c <path> and send-keys
cd "$PROJECTS_DIR/$REPO/cursor" && exec $CURSOR_CMD
cd "$PROJECTS_DIR/$REPO/opencode" && exec $OPENCODE_CMD
```

## Never

- `kill-window` / `kill-pane` on re-sync
- Restart panes that already have a live process
