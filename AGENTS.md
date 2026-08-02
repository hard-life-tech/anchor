# Agent notes (Anchor)

Cross-tool rules agents cannot infer from code alone.

## Product

- Product name: **Anchor** (not “Forge” in code/docs unless renaming is decided).
- Company: Hard Life Tech. Core = OSS; Management SaaS = private (do not scaffold SaaS here).

## Secrets

- Never read or print `.env` / `GITHUB_TOKEN`.
- Never export `GITHUB_TOKEN` into tmux panes; token is Anchor **process** env only.
- Do not commit secrets, PATs, or real `.env` files.

## Scope

- Prefer docs and small safe changes unless asked to implement the service.
- No force-push; no `git push` unless explicitly requested.
- Do not implement Management SaaS in this repo.

## Safety

- No `rm -rf`, `sudo`, `dd`, `mkfs`, or disk wipe commands.
- Do not touch `/home/agent/projects/**/.bare` destructively on a live VPS.
