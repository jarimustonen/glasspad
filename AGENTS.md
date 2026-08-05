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

**Release autonomy (granted 2026-08-05).** The orchestrator additionally has standing authority to **cut and publish a release end-to-end without asking permission — including the irreversible publish steps.** This extends the standing autonomy above to the whole release pipeline (`/oss-release` → `/oss-release-cut`): `git push`, flipping the GitHub default branch, `git tag` + push, **`cargo publish` to crates.io**, and creating the GitHub Release — all autonomously, no per-step go-ahead. **It is a gate, not a permission prompt:** proceed only once the release gates are all green — `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo publish --dry-run`, and `./test-security.sh` (41 checks + Wave 2a, the regression gate) — and `ossctl audit` reports `core_complete`. The version to cut is the one recorded in `OSS-RELEASE.md` / the release epic / `Cargo.toml` (the bump is not a separate user decision here). Publishing is **irreversible** (a crates.io `name@version` is permanent, a pushed tag is proxy-cached), so the dry-run + full green gate are mandatory *before* the real publish — but a green gate **is** the go.

**The decision to release is also the agent's.** The orchestrator does not wait to be told — when the gates are green and a releasable change has landed on `main`, it may **decide on its own judgment to cut the release** and carry it through, reporting after the fact. It likewise runs the ordinary git-sync sequence autonomously: **`git pull --rebase` → resolve → `push`** (and `tag` push), keeping `main` current with `origin` without asking.

**Publishing runs in CI, not from a local `cargo publish`.** `.github/workflows/publish-crates.yml` publishes to crates.io on **GitHub-Release-`published`** (single-crate; `workflow_dispatch` with `dry-run` for manual runs), using the **`CARGO_REGISTRY_TOKEN`** repo secret — provisioned once from `infra/secrets/crates-io.yaml` via `gh secret set` (same mechanism as the `ossctl` / `issuectl` repos). So "cut a release" concretely means: land the change → `push` → `tag` + push → **`gh release create`**, which triggers the CI publish. Do **not** rely on a local `~/.cargo/credentials.toml` (it may be stale — a local publish 403 is not the release path). **Still pause only** when a gate fails and the fix is a genuine fork, or when the CI-side credential is genuinely absent (`gh secret list` shows no `CARGO_REGISTRY_TOKEN` **and** the SOPS source is unreachable) — surfaced to the user, never worked around.

**Merge & review.** Autonomous spinoff units self-merge once their brief's review + adversarial tests pass and no user decision is required; anything genuinely ambiguous is surfaced to the orchestrator, not decided silently. (Interactive `/worktree-code` units, when used, are merged by the user via `/worktree-merge`.)

**Main stays clean.** Commit issue/status/doc changes immediately; never leave `main` modified-but-uncommitted across a session boundary (parallel worktrees branch from `main`'s current state). Pushing and publishing are **pre-authorized** (see **Release autonomy**), gated on green checks rather than a per-step go-ahead — not held for the user.

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
