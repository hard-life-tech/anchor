# Guide — config and secrets

## Load

Read env at startup into a `Config` struct. Missing `GITHUB_TOKEN` or `GITHUB_USER` → fail fast.

## Hygiene

| Do | Don't |
|----|-------|
| Keep token in Anchor process memory | Log token or put in API errors |
| Use for GitHub HTTP + Anchor git fetch | `tmux setenv GITHUB_TOKEN …` |
| Deny `*.env` in coding-agent configs | Commit `.env` |
| Run as user `agent` | Run agents as root |

## Related repo files

- `.env.example` — placeholders only
- `AGENTS.md`, `opencode.json`, `.cursor/permissions.json` — deny env reads for agents editing this repo
- `.cursorignore` — exclude `.env`, `.bare/`
