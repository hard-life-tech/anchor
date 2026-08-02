# Anchor documentation foundation — design

**Date:** 2026-08-02  
**Status:** Implemented in-repo (docs + agent configs)

## Problem

The Anchor workspace had `PROJECT.md`, Dockerfile, and Compose but no documentation tree, product-boundary narrative (OSS vs SaaS), or agent safety configs for contributors/agents editing the repo.

## Goals

1. Professional `docs/` tree (features, conceptual, ADRs, timeline, API, deploy).
2. Encode Hard Life Tech product model: Core OSS, Management SaaS private, self-host + cloud.
3. Root agent safety: short `AGENTS.md`, OpenCode + Cursor permissions, `.cursorignore`.
4. Align naming with `PROJECT.md` (**Anchor**), note informal **Forge** alias.

## Non-goals

- Implementing the Rust service in this change set.
- Creating GitHub org/remote or committing secrets.
- Building Management SaaS code.

## Design summary

- Docs mirror a GameChamps-style IA adapted to Anchor.
- ADRs capture stack, worktrees, tmux, tokens, OSS/SaaS, delivery, shell-out, no-DB, Projects.
- Agent configs deny destructive shell and `*.env` reads; bash defaults to ask.

## Self-review

- No TBD placeholders left in critical paths (org creation called out as blocked TODO).
- Consistent with `PROJECT.md` v1 scope; SaaS marked future/private.
- Naming ambiguity documented in glossary + timeline.
