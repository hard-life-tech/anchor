# ADR-0006 — Self-host and cloud delivery

**Status:** Accepted  
**Date:** 2026-08-02

## Context

Operators want control (self-host on a VPS/Coolify). The company also wants to offer hosted cloud.

## Decision

Ship **one Core engine** for both:

- **Self-host:** Docker Compose / Coolify, Tailscale-only access, operator-supplied PAT.
- **Cloud:** Hard Life Tech runs Core (or fleets of Core) behind private Management SaaS.

v1 documents and implements self-host only; cloud is a delivery mode of the same Core, not a fork of sync logic.

## Consequences

- Compose + Dockerfile are first-class.
- Cloud-specific concerns (multi-tenant routing, billing) stay in private SaaS ([ADR-0005](ADR-0005-oss-saas-split.md)).
