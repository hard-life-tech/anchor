# ADR-0010 — Multi-repo projects (sibling workspace)

**Status:** Accepted  
**Date:** 2026-08-02

## Context

v1 treated each GitHub repo as its own Anchor “project”: one directory, one tmux window, agent cwd locked to that checkout. Operators often need agents to work **across** several related repos (shared libraries, services, docs). UI-only grouping would still leave each pane in a single-repo cwd.

## Decision

1. An **Anchor project** is a named scope with a slug, not a single GitHub repo.
2. **Sibling workspace topology** under `$PROJECTS_DIR/<slug>/`:
   - `.anchor/project.json` — filesystem mirror of metadata
   - `.bares/<owner>__<repo>/` — owner-scoped bare clones (avoids short-name collisions)
   - `cursor/<owner>__<repo>/` and `opencode/<owner>__<repo>/` — agent worktrees
3. Agent pane cwd is the **workspace root** (`…/<slug>/cursor` or `…/opencode`), parent of all member worktrees. Agents `cd` between members.
4. **One tmux window per project slug** (panes: Cursor + OpenCode).
5. Persist projects in **SQLite** (`projects`, `project_repos`) and mirror to `.anchor/project.json` for restart/fs truth.
6. Address repos as **`owner/name`** inside a project (stop relying on ambiguous short names alone).
7. On upgrade, **migrate** legacy `$PROJECTS_DIR/<shortname>/{.bare,cursor,opencode}` into the sibling layout (single member; owner from GitHub `full_name` when possible, else `GITHUB_USER`). Rename tmux windows idempotently without killing live panes.

## Consequences

- Sync/create APIs become project-scoped (`POST /api/projects`, `POST /api/projects/{slug}/sync`, member add/remove).
- Dashboard lists projects (member count, sync rollup), not flat repo=project rows.
- `ADR-0008` remains valid for settings; project metadata now **also** lives in SQLite (see updated note there).
- Out of scope here: N×M panes per repo, prompt relay, SaaS multi-tenant, auto monorepo detection.
