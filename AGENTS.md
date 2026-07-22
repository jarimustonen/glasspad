# Glasspad

AI-friendly scratchpad for rich data views. Lightweight web service that lets
AI agents create and share visual content (dashboards, charts, interactive UIs)
via a simple API.

## Documentation Pattern

Every directory follows this structure:

- `CLAUDE.md` — symlink to `AGENTS.md`
- `AGENTS.md` — all AI-relevant info (consolidated)
- `AGENTS-<TOPIC>.md` — complex topics split out (optional)

## CLI Design Principles

This project follows the AI-first CLI conventions in [`AGENTS-AI-FIRST-CLI.md`](AGENTS-AI-FIRST-CLI.md) — strict input validation, `--json` output, JSONL logs, no interactive prompts, informative errors, composable commands. Read that file before designing or changing CLI surface. The file is a verbatim copy from `homebase`; treat it as shared canon, not a project-local doc to edit.

## Gitignored directories

- `history/` — agent scratchpad and ephemeral planning docs (not tracked)
- `.worktree/` — agent worktree checkouts (not tracked)

## Issues & Planning

Work is tracked as issues in `issues/`. Use `/issue` to create, search, update, and close issues.

- `issues/open/NN-slug/item.md` — active issues
- `issues/closed/NN-slug/item.md` — completed issues
- `issues/AGENTS.md` — templates, types, and workflow docs

All planning documents (plans, analyses, designs, todos) belong under their parent issue directory — not as standalone files. If work needs a planning document, it also needs an issue. This ties every piece of planning to a trackable item.

- `issues/open/NN-title/plan.md` — architecture, implementation plans
- `issues/open/NN-title/analysis.md` — research and analysis
- `issues/open/NN-title/design.md` — design documents
- `issues/open/NN-title/todo.md` — task checklists

## Debugging rendered output

Charts render client-side with Vega-Lite. JS is embedded at build time —
after editing `src/client/dashboard.js`: `cargo build`, restart server,
create a new pad.

Use `./test-browser.sh` for browser automation (requires Brave > View >
Developer > Allow JavaScript from Apple Events). Always check
`./test-browser.sh errors` first.

Full debugging guide: **[AGENTS-GUI-DEBUGGING.md](AGENTS-GUI-DEBUGGING.md)**
