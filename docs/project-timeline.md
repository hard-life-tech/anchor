# Project timeline and TODOs

Status as of 2026-08-02. v1 scope is defined in [`PROJECT.md`](../PROJECT.md).

## Phases

| Phase | Name | Goal | Status |
|-------|------|------|--------|
| **0** | Docs + agent safety | Docs tree, ADRs, agent permission configs | Done |
| **1** | Core scaffold | Rust crate, axum routes, git/tmux modules, Docker build | In progress |
| **2** | MVP acceptance | Meet §14 acceptance criteria in `PROJECT.md` | In progress |
| **3** | Hardening | Secrets audit, non-root verification, rate-limit cache | Not started |
| **4** | Open source launch | Public repo under Hard Life Tech, LICENSE, CONTRIBUTING | In progress |
| **5** | Cloud / SaaS (private) | Multi-tenant management product on top of Core | Future |

## Milestone checklist

### Phase 0 — Docs foundation
- [x] `docs/` tree matching conceptual / features / ADR layout
- [x] Root agent safety files (`AGENTS.md`, `opencode.json`, `.cursor/*`, `.cursorignore`)
- [x] Confirm product branding: **Anchor** (Forge = historical alias only)
- [x] Create public `hard-life-tech/anchor` repo
- [x] Choose LICENSE for Core: **Apache-2.0**

### Phase 1 — Core scaffold (priority order)
1. [x] Cargo workspace / binary `anchor` matching Dockerfile `COPY` path
2. [x] Config from env (`GITHUB_TOKEN`, `GITHUB_USER`, `PROJECTS_DIR`, …)
3. [x] Git module: bare clone, worktree add, fetch, fast-forward-only update
4. [x] Tmux module: session/window/pane ensure (idempotent, never kill live panes)
5. [x] GitHub client: list repos (in-memory cache)
6. [x] HTTP API per [api-contract.md](api-contract.md)
7. [x] Askama + htmx dashboard
8. [x] Docker image builds; container smoke (`/healthz`, `/`) — full Compose with real `.env` still operator-side

### Phase 2 — MVP acceptance
- [x] Fresh sync creates `.bare` + both worktrees + tmux window (unit-covered; Compose e2e pending)
- [x] Re-sync is idempotent (no duplicate windows, no force overwrite) (unit-covered)
- [x] Dirty/diverged worktrees reported, left untouched (dirty covered; diverged path present)
- [x] `/api/projects` lists on-disk projects without tmux (restart-safe inventory unit-tested); Compose restart e2e still needs live PAT — [#1](https://github.com/hard-life-tech/anchor/issues/1)
- [x] Healthz works in container image (Compose with live PAT still operator-side — [#2](https://github.com/hard-life-tech/anchor/issues/2))

### Phase 3 — Hardening
- [x] Assert `GITHUB_TOKEN` never present in agent pane / session env (scrub + unit tests; Compose confirm — [#3](https://github.com/hard-life-tech/anchor/issues/3))
- [x] Deny-path tests for API errors (PAT shapes redacted); tracing audit still open — [#4](https://github.com/hard-life-tech/anchor/issues/4)
- [ ] Document Tailscale / Coolify production checklist — [#5](https://github.com/hard-life-tech/anchor/issues/5)
- [ ] Optional: drop Compose host port publish in prod examples — [#6](https://github.com/hard-life-tech/anchor/issues/6)

### Phase 4 — OSS launch
- [x] `gh` auth available for Hard Life Tech
- [x] Remote + README
- [x] CONTRIBUTING + basic issue templates
- [x] GitHub Projects board for Core roadmap — [project-management.md](conceptual/workflow/project-management.md) / https://github.com/orgs/hard-life-tech/projects/4

### Phase 5 — Management SaaS (private)
- [ ] Separate private repo / distribution channel
- [ ] Multi-tenant control plane calling Core APIs or embedding Core
- [ ] Billing, SSO, fleet policy — out of OSS tree

## Sprint-style backlog (next 2 weeks)

| ID | Task | Priority | Depends |
|----|------|----------|---------|
| T01 | Finalize naming (Anchor) in public messaging | P0 | Done |
| T02 | Scaffold `Cargo.toml` + `src/main.rs` hello healthz | P0 | Done |
| T03 | Implement config + tracing | P0 | Done |
| T04 | Implement git worktree sync (TDD) | P0 | Done |
| T05 | Implement tmux ensure (TDD) | P0 | Done |
| T06 | Wire sync API + GitHub list | P1 | Done |
| T07 | Minimal dashboard | P2 | Done |
| T08 | Compose e2e on VPS | P1 | T06 |
| T09 | Create GH org/repo when `gh` ready | P1 | Done |
| T10 | Publish OSS LICENSE + CONTRIBUTING | P2 | Done |

## Out of scope (tracked, not scheduled)

- GitHub App (replace PAT)
- Webhook auto-sync
- Built-in web terminal (vs ttyd)
- Dashboard login (vs Tailscale trust)
- Multi-user / multi-tenant Core
