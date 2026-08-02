# F-002 — Project sync (multi-repo sibling workspace)

**Phase:** MVP  
**API:** `POST /api/projects/{slug}/sync` (and member add/remove)

## Intent

Materialize one or more GitHub repos under a named Anchor project as owner-scoped bare clones plus agent worktrees, or update without destroying agent work. Agents share a workspace root so they can work across members.

## Layout

```
$PROJECTS_DIR/<slug>/
  .anchor/project.json
  .bares/<owner>__<repo>/
  cursor/<owner>__<repo>/      # branch agent/cursor
  opencode/<owner>__<repo>/    # branch agent/opencode
```

Pane cwd = `cursor/` or `opencode/` (parent of siblings). See [ADR-0010](../conceptual/adr/ADR-0010-multi-repo-projects.md).

## Rules

- First sync per member: `git clone --bare` into `.bares/<owner>__<repo>/`, then `worktree add` for both agents from `origin/<default>`.
- Later sync: `fetch`; fast-forward each worktree only if clean and not diverged.
- **Never** force-overwrite dirty or diverged worktrees — report and skip.
- Removing a member does not force-delete dirty worktrees.
- Idempotent: safe to call repeatedly.
- After git steps, ensure tmux window named by **slug** (F-003).
- HTTPS auth uses process-local `Authorization: Basic` via `GIT_CONFIG_*` (not token-in-URL, not argv). Never log the token.
- Legacy 1:1 dirs migrate into this layout on startup / sync.

## Acceptance

- [ ] Fresh project sync creates bares + both worktrees per member
- [ ] Re-sync does not duplicate worktrees or clobber dirty state
- [ ] Response lists per-worktree `action` including skip reasons
- [x] Private/enterprise repos authenticate without TTY username prompts
- [ ] Legacy layout migrates without data loss for clean trees
