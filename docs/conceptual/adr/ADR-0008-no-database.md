# ADR-0008 — No database in v1

**Status:** Accepted  
**Date:** 2026-08-02

## Context

Orchestration state already lives in git and tmux. A DB would duplicate truth and add backup/migration burden for a single-operator tool.

## Decision

**No persistent database.** Query filesystem, git, and tmux on demand. Optional short-lived in-memory cache for GitHub repo lists only.

## Consequences

- Simpler ops and restarts (except live tmux loss).
- SaaS multi-tenant metadata later belongs in private Management services, not Core v1.
