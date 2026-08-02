# Operator day workflow

1. Open Anchor dashboard over Tailscale on phone.
2. Pick a GitHub repo → **Sync**.
3. Anchor ensures bare clone, worktrees, and tmux window.
4. Attach via SSH (`tmux attach -t agents`) or ttyd sidecar.
5. Prompt Cursor and/or OpenCode in their panes (separate branches).
6. Commit/push from each worktree as needed; merge via normal PR flow.
7. Re-sync later to fetch upstream; dirty worktrees are skipped, not forced.

Anchor does not build, test, or deploy the synced repos (not CI/CD).
