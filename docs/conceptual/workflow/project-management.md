# Project management (GitHub Projects)

Anchor Core will be managed as an open-source project under **Hard Life Tech**.

## Recommended setup (when org exists)

1. Public repo `hard-life-tech/anchor` (name TBD with branding).
2. GitHub **Projects** (or Enterprise Projects if the org is on GHE) board with columns aligned to [project-timeline.md](../../project-timeline.md):
   - Backlog → Ready → In progress → Review → Done
3. Map Phase 1–3 checklist items to issues; label `mvp`, `security`, `docs`, `saas-private`.
4. Keep **SaaS / Management** work in a **private** project/repo so OSS contributors do not see closed-source roadmaps.

## Conventions

- One issue ≈ one shippable slice (see timeline backlog IDs T01…).
- ADRs linked from issues that change architecture.
- Do not put secrets or customer cloud details in public project notes.

## Status

**Blocked:** GitHub org/repo not created yet. Track under Phase 4 in the timeline.
