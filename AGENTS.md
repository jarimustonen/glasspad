# Glasspad

AI-friendly scratchpad for rich data views. Lightweight web service that lets
AI agents create and share visual content (dashboards, charts, interactive UIs)
via a simple API.

## Planning & History

AI-generated planning documents go in `history/` at the repo root.

- `history/TODO.md` -- master-työsuunnitelma
- `history/plan-<topic>.md` -- planning documents
- `history/analysis-<topic>.md` -- research and analysis
- `history/design-<topic>.md` -- design documents
- `history/review-<topic>.md` -- review and audit documents

## Debugging rendered output

Charts render client-side with Vega-Lite. JS is embedded at build time —
after editing `src/client/dashboard.js`: `cargo build`, restart server,
create a new pad.

Use `./test-browser.sh` for browser automation (requires Brave > View >
Developer > Allow JavaScript from Apple Events). Always check
`./test-browser.sh errors` first.

Full debugging guide: **[AGENTS-GUI-DEBUGGING.md](AGENTS-GUI-DEBUGGING.md)**
