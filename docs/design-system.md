# Design system (dashboard)

v1 UI is a **small server-rendered dashboard**, not a SPA. Stack: **Askama** templates + **htmx** for light interactivity (per `PROJECT.md`).

## Goals

- Usable on a phone browser over Tailscale
- Trigger sync and see project/worktree/tmux status at a glance
- Zero frontend build step

## Non-goals (v1)

- Design-system package / component library
- Dark-mode theming product
- Marketing / landing pages in this binary
- Cards-heavy “AI dashboard” chrome

## Visual direction

Keep the first viewport simple: product name **Anchor**, one short status line, project list, primary actions.

| Token | Suggested value | Role |
|-------|-----------------|------|
| `--bg` | `#0f1419` or `#f6f7f8` | Page background (pick one mode for v1; prefer a single clear theme) |
| `--fg` | high-contrast text | Body |
| `--accent` | `#2a6f5e` (teal-green) | Primary actions — avoid purple-gradient clichés |
| `--danger` | `#b33a3a` | Dirty / diverged flags |
| `--muted` | secondary text | Meta (ahead/behind) |
| `--font-sans` | `"IBM Plex Sans", "Source Sans 3", sans-serif` | UI |
| `--font-mono` | `"IBM Plex Mono", "JetBrains Mono", monospace` | Branches, paths |

Atmosphere: subtle top gradient or soft noise — not a flat white void, not glow stacks.

## Layout

1. **Header:** wordmark “Anchor” + health indicator.
2. **Actions:** refresh repos / sync selected (htmx `POST`).
3. **List:** one row per project — on-disk, worktree dirty/diverged chips, tmux window yes/no, Sync button.
4. **Empty state:** one sentence + link to sync from repo list.

No hero marketing. No card grid for status — rows/tables are fine because they are the interaction surface.

## Motion

- htmx swap fade (~150ms) on status refresh
- Brief button busy state during sync
- No decorative looping animations

## Accessibility

- Buttons are real `<button>` / links
- Status colors paired with text labels (`dirty`, `diverged`)
- Target sizes comfortable for thumb (min ~44px)

## Future (SaaS Management UI)

Private Management SaaS may ship a separate frontend. It must not be required to run OSS Core. Keep Askama dashboard in Core forever as the zero-deps operator UI.
