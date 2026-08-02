# ADR-0007 — Shell out to git and tmux

**Status:** Accepted  
**Date:** 2026-08-02

## Context

Rust git bindings (`git2`) have inconsistent worktree support. tmux has no valuable Rust crate vs the CLI.

## Decision

Invoke real **`git`** and **`tmux`** binaries via `tokio::process::Command`. Bundle them in the runtime image.

## Consequences

- Behavior matches operator mental model and docs/scripts.
- Depends on CLI availability/version in the image.
- Easier to debug (same commands humans run).
