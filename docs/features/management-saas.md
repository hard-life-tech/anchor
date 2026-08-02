# F-020 — Management SaaS (private)

**Phase:** SaaS (company-distributed, **not** OSS)

## Intent

Hard Life Tech–operated cloud and multi-tenant management around Anchor Core: fleets, billing, org policy, SSO, hosted instances.

## Boundary

- Lives in a **private** distribution channel / repo.
- May orchestrate or embed Core; must not require closed-source code to run self-hosted Core.
- See [ADR-0005](../conceptual/adr/ADR-0005-oss-saas-split.md).

## Non-goals for Core

- Multi-tenant tenancy tables in the OSS binary
- Billing code in OSS
