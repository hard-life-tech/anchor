# ADR-0004 — Token handling

**Status:** Accepted  
**Date:** 2026-08-02

## Context

Anchor needs a GitHub PAT (`repo` scope) to list and clone repos. Agent CLIs have their own auth. Leaking the PAT into agent environments would expand blast radius (agents run arbitrary tools).

## Decision

1. Single fine-grained **PAT** in v1 (GitHub App later).
2. `GITHUB_TOKEN` exists only in the **Anchor process environment**.
3. **Never** export the token into agent tmux panes; never log or return it.
4. Coding agents on this repo are denied `*.env` via `AGENTS.md` / OpenCode / Cursor configs.
5. Agent CLIs use their own OAuth/device flows; Anchor does not proxy them.

## Consequences

- Git operations that need auth must be performed by Anchor (or with scoped askpass), not by hoping the pane has the token.
- Operators must not manually `export GITHUB_TOKEN` in panes.
