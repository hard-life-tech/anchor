# Guide — git worktrees

## Create

```bash
git clone --bare <clone_url> "$PROJECTS_DIR/$REPO/.bare"
git -C "$PROJECTS_DIR/$REPO/.bare" worktree add ../cursor -b agent/cursor "origin/$DEFAULT"
git -C "$PROJECTS_DIR/$REPO/.bare" worktree add ../opencode -b agent/opencode "origin/$DEFAULT"
```

## Update

```bash
git -C .bare fetch origin
# per worktree: if dirty or not ff-able → skip; else
git -C cursor merge --ff-only "origin/$DEFAULT"
```

## Auth for HTTPS

Prefer passing credentials only for Anchor-invoked git commands (e.g. `Authorization` header via `GIT_ASKPASS` / `http.extraHeader` scoped to that process), without writing the PAT into the agent worktrees' local git config if avoidable. Never put the token in tmux pane env.

## Safety

- Idempotent worktree add: detect existing worktree paths.
- Never `reset --hard` or force checkout on operator dirt.
