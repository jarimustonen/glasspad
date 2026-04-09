# Glasspad

AI-friendly scratchpad for rich data views. Lightweight web service that lets
AI agents create and share visual content (dashboards, charts, interactive UIs)
via a simple API.

## Issues & Planning

Work is tracked as issues in `issues/` at the repo root. Use `/issue` to
create new epics, tasks, and bugs.

- `issues/open/NN-slug/item.md` — active issues
- `issues/closed/NN-slug/item.md` — completed issues
- `issues/AGENTS.md` — templates and structure docs

Planning docs (`plan.md`, `analysis.md`, `design.md`) belong under their
parent issue directory, not as standalone files.

### Gitignored directories

- `history/` — legacy planning docs and agent scratchpad (not tracked)
- `.worktree/` — agent worktree checkouts (not tracked)

## Debugging rendered output

Charts render client-side with Vega-Lite. JS is embedded at build time —
after editing `src/client/dashboard.js`: `cargo build`, restart server,
create a new pad.

Use `./test-browser.sh` for browser automation (requires Brave > View >
Developer > Allow JavaScript from Apple Events). Always check
`./test-browser.sh errors` first.

Full debugging guide: **[AGENTS-GUI-DEBUGGING.md](AGENTS-GUI-DEBUGGING.md)**
