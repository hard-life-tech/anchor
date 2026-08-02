# Workflows

| Doc | Description |
|-----|-------------|
| [operator-day.md](operator-day.md) | Typical day: sync from phone, attach, prompt agents |
| [project-management.md](project-management.md) | GitHub Projects / Enterprise Projects usage for this repo |

## Operator flow (summary)

```mermaid
flowchart LR
  Phone[Phone browser] --> Dash[Anchor dashboard]
  Dash -->|POST sync| Core[Anchor Core]
  Core --> Disk[Worktrees on volume]
  Core --> Tmux[tmux panes]
  Op[Operator] -->|SSH or ttyd| Tmux
  Tmux --> Cursor[cursor-agent]
  Tmux --> OC[opencode]
```
