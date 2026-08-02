# Guide — git worktrees

## Layout (multi-repo project)

```
$PROJECTS_DIR/<slug>/
  .bares/<owner>__<repo>/
  cursor/<owner>__<repo>/
  opencode/<owner>__<repo>/
```

## Create (per member)

```bash
KEY="${OWNER}__${REPO}"
git clone --bare <clone_url> "$PROJECTS_DIR/$SLUG/.bares/$KEY"
git -C "$PROJECTS_DIR/$SLUG/.bares/$KEY" worktree add "../../cursor/$KEY" -b agent/cursor "origin/$DEFAULT"
git -C "$PROJECTS_DIR/$SLUG/.bares/$KEY" worktree add "../../opencode/$KEY" -b agent/opencode "origin/$DEFAULT"
```

Agent pane cwd is `$PROJECTS_DIR/$SLUG/cursor` (or `opencode`), not a single member path.

## Update

```bash
git -C .bares/$KEY fetch origin
# per worktree: if dirty or not ff-able → skip; else
git -C "cursor/$KEY" merge --ff-only "origin/$DEFAULT"
```

## Auth for HTTPS

Prefer passing credentials only for Anchor-invoked git commands (e.g. `Authorization` header via `GIT_CONFIG_*` / `http.extraHeader` scoped to that process), without writing the PAT into the agent worktrees' local git config if avoidable. Never put the token in tmux pane env.

## Safety

- Idempotent worktree add: detect existing worktree paths.
- Never `reset --hard` or force checkout on operator dirt.
- Owner-scoped keys (`owner__repo`) avoid short-name collisions across orgs.
