# F-011 — Webhook auto-sync (later)

On `push` to default branch, trigger the same idempotent sync as `POST /api/projects/{repo}/sync`. Still must not force dirty/diverged worktrees.

**Depends on:** F-002, public or tunneled webhook endpoint + auth story.
