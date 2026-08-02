# ADR-0005 — OSS Core vs private SaaS split

**Status:** Accepted  
**Date:** 2026-08-02

## Context

Hard Life Tech wants an open-source engine and a company-distributed management product (cloud, multi-tenant, billing).

## Decision

- **Anchor Core** (this repo): open source — git sync, worktrees, tmux, HTTP API, single-operator dashboard.
- **Management SaaS**: private — tenancy, billing, SSO, hosted fleets, org policy.
- Self-hosted Core must remain fully usable without SaaS.
- Do not merge SaaS-only code into the public tree; use a separate private distribution.

## Consequences

- Clear contribution boundary for OSS.
- Duplicate packaging possible (public image vs managed image) — keep Core APIs stable as the integration surface.
- Branding/org setup still TODO.
