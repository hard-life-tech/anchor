# F-002 — Project sync (bare + worktrees)

**Phase:** MVP  
**API:** `POST /api/projects/{repo}/sync`

## Intent

Materialize a GitHub repo on disk as a bare clone plus two agent worktrees, or update an existing project without destroying agent work.

## Layout

```
$PROJECTS_DIR/<repo>/
  .bare/
  cursor/      # branch agent/cursor
  opencode/    # branch agent/opencode
```

## Rules

- First sync: `git clone --bare`, then `worktree add` for both agents from `origin/<default>`.
- Later sync: `fetch`; fast-forward each worktree only if clean and not diverged.
- **Never** force-overwrite dirty or diverged worktrees — report and skip.
- Idempotent: safe to call repeatedly.
- After git steps, ensure tmux (F-003).

## Acceptance

- [ ] Fresh sync creates bare + both worktrees
- [ ] Re-sync does not duplicate worktrees or clobber dirty state
- [ ] Response lists per-worktree `action` including skip reasons
