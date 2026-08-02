# Anchor documentation

**Anchor** is a Rust service that clones/syncs GitHub repos onto a VPS and wires each repo to two AI coding agents (Cursor CLI and OpenCode) in tmux — so sessions stay persistently *anchored* to the host and can be driven from a phone.

| | |
|---|---|
| **Company** | Hard Life Tech ([hard-life-tech](https://github.com/hard-life-tech)) |
| **Product name** | **Anchor** (workspace dir may still be called `anchor`; see [glossary](glossary.md)) |
| **License model** | Core engine = open source; management SaaS = private (company-distributed) |
| **Delivery** | Self-hosted **and** Hard Life Tech cloud |

Primary source of truth for v1 behavior: [`PROJECT.md`](../PROJECT.md) at the repo root.

## How to use this tree

| Doc | Purpose |
|-----|---------|
| [glossary.md](glossary.md) | Terms (Anchor, worktree, panes, PAT, OSS/SaaS boundary) |
| [project-timeline.md](project-timeline.md) | Phases, milestones, prioritized TODOs |
| [api-contract.md](api-contract.md) | HTTP API for v1 |
| [deployment-guide.md](deployment-guide.md) | Self-host (Docker/Coolify) and cloud notes |
| [design-system.md](design-system.md) | Dashboard UI (Askama + htmx) |
| [features/@index.md](features/@index.md) | Feature specs (MVP → later) |
| [conceptual/@index.md](conceptual/@index.md) | Architecture, workflows, ADRs |
| [superpowers/](superpowers/) | Design specs and implementation plans |
| [_archive/](_archive/) | Retired docs |

## Product boundary (OSS vs SaaS)

```
┌─────────────────────────────────────────────────────────┐
│  Anchor Core (open source)                              │
│  git sync · worktrees · tmux orchestration · HTTP API   │
│  local dashboard · single-operator self-host            │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼  (optional, company product)
┌─────────────────────────────────────────────────────────┐
│  Anchor Management (private / SaaS)                     │
│  multi-tenant · billing · org policies · hosted cloud   │
│  fleet dashboards · managed agents · enterprise SSO     │
└─────────────────────────────────────────────────────────┘
```

v1 ships **only Core** for a single operator on a private network (Tailscale). SaaS features are documented as future phases — they must not leak into the OSS binary without an explicit boundary (see [ADR-0005](conceptual/adr/ADR-0005-oss-saas-split.md)).

## Quick links

- [Sync + tmux flow](conceptual/architecture/diagrams/flows/sync-and-tmux.md)
- [Sync sequence](conceptual/architecture/diagrams/sequences/sync-project.md)
- [Token / agent env isolation](conceptual/architecture/diagrams/flows/secret-isolation.md)
- [Implementation plan (scaffold)](superpowers/plans/2026-08-02-anchor-core-scaffold.md)

## Agent safety

Repo-root agent configs (`AGENTS.md`, `opencode.json`, `.cursor/`) constrain coding agents working *on* Anchor itself. Runtime agents that Anchor *launches* are separate — see [ADR-0004](conceptual/adr/ADR-0004-token-handling.md).
