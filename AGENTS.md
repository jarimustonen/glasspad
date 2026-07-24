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

Work is tracked with **`issuectl`** in a flat, slug-based layout. Use `/issue` (or `issuectl` directly) to create, search, update, and close issues. `issuectl doctor` health-checks the tree.

- `issues/<slug>/item.md` — one issue (status lives in frontmatter, not the directory)
- `issues/.schema.yaml` — frontmatter schema (types, statuses, priorities)
- `issues/AGENTS.md` + `.issuectl/AGENTS.md` — workflow and agent policy docs

Slugs are descriptive kebab-case (`html-artifact-host-rewrite`), never numbered. Create with `issuectl new --slug <2-3-word-kebab> …`.

All planning documents (plans, analyses, designs, todos) belong under their parent issue directory — not as standalone files. If work needs a planning document, it also needs an issue. This ties every piece of planning to a trackable item.

- `issues/<slug>/plan.md` — architecture, implementation plans
- `issues/<slug>/analysis.md` — research and analysis
- `issues/<slug>/design.md` — design documents

The repo-root `TODO.md` is the round-by-round handoff for `/stint`; the issue tracker is the source of truth for detail.

## Operating Policy

Generic operating policy for orchestrated work sessions (`/stint`) and worktrees.

**Roles.** A `/stint` session is the **orchestrator** the user talks to in product-owner language. It plans rounds, triages incoming bugs, reports status, and owns the single local deploy — it **does not write code in its own session**. Actual coding happens in worktrees spawned via `/worktree` (interactive `/worktree-code` for reviewed work, `/worktree-spinoff` for autonomous units).

**Pre-authorized in any worktree** (no need to ask): read anything, `cargo build`, `cargo test`, `cargo clippy`, and `./test-browser.sh` (check `./test-browser.sh errors` first). Verifying a change end-to-end before merge is expected, not optional.

**Deploy = localhost.** There is no remote deploy. "Deploy/verify" means: `cargo build`, `glasspad serve` a space, confirm the sandboxed artifact renders in the browser, and run `./test-security.sh` (41 checks + Wave 2a probes) as the regression gate. The orchestrator owns this single build+verify when integrating a round.

**Standing autonomy (granted 2026-07-24).** The orchestrator has standing authority to **push planned work forward without asking permission** — spawn worktrees, merge landed units, deploy to localhost, and start the next unit, all autonomously. Work should keep moving; do not pause for go/no-go on routine spawns, merges, or the localhost deploy. **Prefer spinoffs** (`/worktree-spinoff --headless`, self-merging) over interactive `/worktree-code`, with `/llm-review` (+ `/assess-findings`) in the brief for any unit that touches production/security code. Still pause only for: a genuine fork where reasonable people disagree, something that cannot be done, or a bug fix/defer/not-a-bug decision (always the user's call). (Originally granted to drive the v0.2 HTML-artifact-host rewrite, which completed 2026-07-24; the posture carries forward to subsequent rounds.)

**Merge & review.** Autonomous spinoff units self-merge once their brief's review + adversarial tests pass and no user decision is required; anything genuinely ambiguous is surfaced to the orchestrator, not decided silently. (Interactive `/worktree-code` units, when used, are merged by the user via `/worktree-merge`.)

**Main stays clean.** Commit issue/status/doc changes immediately; never leave `main` modified-but-uncommitted across a session boundary (parallel worktrees branch from `main`'s current state). Pushing is the user's call.

**CLI surface** follows the AI-first conventions above: strict validation, `--json`, no interactive prompts, informative errors.

## Debugging rendered output

Glasspad is an **HTML-artifact host** (v0.2): the calling agent authors HTML in
a directory and `glasspad serve ./dir` hosts each file in a null-origin
sandboxed iframe. There is no content-DSL and no server-side renderer — the old
`src/spec/*` + `src/client/dashboard.js` path was removed. The host internals
live in **`src/artifact_host/`** (see its `AGENTS.md`).

Base libraries are served under `/_gp/v1/`: `base.css` (the `--gp-*` design
system), `charts.js` (`gp.chart(el, spec)` over Vega-Lite), `bridge.js`
(auto-injected same-space nav + theme), `manifest.json`. After changing host
code or a base lib: `cargo build`, restart `glasspad serve`, reload the space.

**The security contract is the gate.** `./test-security.sh` is a self-contained
Playwright suite (41 adversarial browser checks + Wave 2a space-model probes:
per-channel exfil, sandbox-escape, direct-open, postMessage abuse, traversal/
symlink, injection, vega/eval). Run it after any change to the host, headers,
CSP, or bridge — it must stay green. Legacy data formats (CSV/JSON/mbox) parse
via the optional `glasspad data` CLI helper, not the host.

Use `./test-browser.sh` for ad-hoc browser automation (requires Brave > View >
Developer > Allow JavaScript from Apple Events). Always check
`./test-browser.sh errors` first.

Full debugging guide: **[AGENTS-GUI-DEBUGGING.md](AGENTS-GUI-DEBUGGING.md)**
