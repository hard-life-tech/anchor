# Glossary

| Term | Meaning |
|------|---------|
| **Anchor** | The product and Rust binary: GitHub → worktrees → tmux agents orchestrator. Named because agent sessions stay *anchored* to the VPS. |
| **Forge** | Informal / alternate name from early planning. **Canonical product name is Anchor.** Retained here only as historical alias. |
| **Hard Life Tech** | Company / open-source org for Anchor Core (`hard-life-tech` on GitHub). |
| **Core (OSS)** | Open-source engine: clone/fetch, worktrees, tmux panes, HTTP API, single-operator dashboard. |
| **Management SaaS** | Private, company-distributed features: multi-tenant cloud, billing, org policy, fleets. Not in v1. |
| **Operator** | The single human running an Anchor instance (v1 is single-operator, not multi-tenant). |
| **Project** | A GitHub repo mirrored under `PROJECTS_DIR/<repo>/` with `.bare/`, `cursor/`, `opencode/`. |
| **`.bare/`** | Bare git clone — source of truth for history. Not a working tree. |
| **Worktree** | Linked working directory (`cursor/` or `opencode/`) on branch `agent/cursor` or `agent/opencode`. |
| **Pane** | A tmux pane running one agent CLI (`cursor-agent` or `opencode`). |
| **Window** | One tmux window per repo inside the shared session (`TMUX_SESSION`, default `agents`). |
| **PAT** | GitHub fine-grained personal access token (`GITHUB_TOKEN`), `repo` scope only in v1. |
| **Anchor process env** | Environment of the Anchor *service* process — where `GITHUB_TOKEN` lives. Must **not** be exported into agent tmux panes. (Formerly called “Forge process env” in drafts.) |
| **Agent user** | Non-root OS user (`agent`, uid 1000 by default) that runs Anchor and agent CLIs inside the container. |
| **Coolify** | Deployment target for self-host Compose resources (behind Traefik/Tailscale). |
| **ttyd** | Optional sidecar for browser attach to the tmux session; not part of Anchor's HTTP API in v1. |
