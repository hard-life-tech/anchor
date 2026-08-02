# ADR-0002 — Git worktree strategy

**Status:** Accepted  
**Date:** 2026-08-02

## Context

Two AI agents must work on the same GitHub repo concurrently without clobbering each other's uncommitted changes.

## Decision

Use a **bare clone** (`.bare/`) plus **two linked worktrees** (`cursor/`, `opencode/`) on branches `agent/cursor` and `agent/opencode`. Sync only fast-forwards; never force dirty/diverged trees.

## Consequences

- Safe parallelism; merge via normal git/PR.
- Slightly more complex than a shared working directory.
- Disk uses one object store + two checkouts.
