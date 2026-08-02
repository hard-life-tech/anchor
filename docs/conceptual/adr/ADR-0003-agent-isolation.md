# ADR-0003 — Agent isolation via tmux

**Status:** Accepted  
**Date:** 2026-08-02

## Context

Operators need persistent agent CLIs reachable from phone/SSH. Anchor should not become a full IDE or PTY proxy in v1.

## Decision

One shared tmux session (`TMUX_SESSION`), one window per repo, two panes (Cursor CLI, OpenCode). Anchor ensures panes exist; operators attach for prompting. Re-sync must never kill live panes.

## Consequences

- Simple, battle-tested process supervision.
- Container restart drops tmux (accepted v1 tradeoff); disk auth remains.
- Prompt relay / embedded terminal deferred (F-013).
