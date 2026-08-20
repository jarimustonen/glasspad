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

Use the `/ai-first-cli-canon` skill shipped by `project-canon` as the maintained AI-first CLI canon. It is the binding reference for CLI surface work: strict input validation, `--json` output, JSONL logs, no interactive prompts, informative errors and composable commands. Do not keep or edit a repo-local `AGENTS-AI-FIRST-CLI.md` copy; update the canon in the `project-canon` source package and reinstall the skill from the released tool.


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

**Standing autonomy (granted 2026-07-24).** The orchestrator has standing authority to **push planned work forward without asking permission** — spawn worktrees, merge landed units, deploy to localhost, and start the next unit, all autonomously. Work should keep moving; do not pause for go/no-go on routine spawns, merges, or the localhost deploy. **Prefer spinoffs** (`/worktree-spinoff --headless`, self-merging) over interactive `/worktree-code`, with `/llm-review` (+ `/assess-findings`) in the brief for any unit that touches production/security code. Still pause only for: a genuine fork where reasonable people disagree, something that cannot be done, or a bug fix/defer/not-a-bug decision (always the user's call). (Originally granted to drive the v0.2 HTML-artifact-host rewrite, which completed 2026-07-24; the posture carries forward to subsequent rounds.)

**Release autonomy (granted 2026-08-05).** The orchestrator additionally has standing authority to **cut and publish a release end-to-end without asking permission — including the irreversible publish steps.** This extends the standing autonomy above to the whole release pipeline (`/oss-release` → `/oss-release-cut`): `git push`, flipping the GitHub default branch, `git tag` + push, **`cargo publish` to crates.io**, and creating the GitHub Release — all autonomously, no per-step go-ahead. **It is a gate, not a permission prompt:** proceed only once the release gates are all green — `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo publish --dry-run`, and `./test-security.sh` (41 checks + Wave 2a, the regression gate) — and `ossctl audit` reports `core_complete`. The version to cut is the one recorded in `OSS-RELEASE.md` / the release epic / `Cargo.toml` (the bump is not a separate user decision here). Publishing is **irreversible** (a crates.io `name@version` is permanent, a pushed tag is proxy-cached), so the dry-run + full green gate are mandatory *before* the real publish — but a green gate **is** the go.

**The decision to release is also the agent's.** The orchestrator does not wait to be told — when the gates are green and a releasable change has landed on `main`, it may **decide on its own judgment to cut the release** and carry it through, reporting after the fact. It likewise runs the ordinary git-sync sequence autonomously: **`git pull --rebase` → resolve → `push`** (and `tag` push), keeping `main` current with `origin` without asking.

**No permission is needed for a release — do NOT ask (reaffirmed 2026-08-11).** A green gate plus a releasable change on `main` **is itself the authorization**; decide and cut the release, then report after the fact. Do **not** surface "shall I cut 0.X.0?" / "leikataanko release?" as an open decision, do not park a landed-but-unreleased change waiting on the user, and do not treat "the API surface is new/irreversible" as a reason to hold — the mandatory dry-run + full green gate already cover irreversibility, and a green gate is the go. The user asking to "continue developing" does not suspend this: if releasable work is sitting green on `main`, releasing it is part of moving the work forward. The **only** release pauses are the ones already enumerated in **Release autonomy**: a gate fails and its fix is a genuine fork, or the CI-side crates.io credential is genuinely absent — never a routine go/no-go.

**Publishing runs in CI, not from a local `cargo publish`.** Both release workflows are triggered by the **version-tag push itself** (`push: tags: v[0-9]+.[0-9]+.[0-9]+*`), NOT by a `release: published` event: `.github/workflows/publish-crates.yml` publishes to crates.io (using the **`CARGO_REGISTRY_TOKEN`** repo secret; `workflow_dispatch` with `dry-run` for manual runs), and `.github/workflows/release.yml` (cargo-dist) builds the binaries, **creates the GitHub Release itself** via `GITHUB_TOKEN`, and pushes the Homebrew formula. (publish-crates keys off the tag precisely *because* a Release created by `GITHUB_TOKEN` emits no `release` workflow event.) The secret is provisioned once as a repository secret via `gh secret set`. So "cut a release" concretely means: land the change → `push` main → **`tag` + push the tag** — the tag push alone triggers both workflows; **no `gh release create` step is needed or wanted** (cargo-dist owns Release creation). Do **not** use the generic `ossctl release cut` / `/oss-release` cut engine here — its `publish-all` phase does a *local* `cargo publish`, which this policy forbids. Do **not** rely on a local `~/.cargo/credentials.toml` (it may be stale — a local publish 403 is not the release path). Confirmed by the 0.4.0 cut (2026-08-10): a single tag push published all three channels. **Still pause only** when a gate fails and the fix is a genuine fork, or when the CI-side credential is genuinely absent (`gh secret list` shows no `CARGO_REGISTRY_TOKEN` and no approved secret source is available) — surfaced to the user, never worked around.

**Cross-platform is a hard requirement (macOS AND Linux).** glasspad MUST install and run on **both macOS and Linux** — a release path that works on only one OS is incomplete, matching the `/oss-*` family canon (see `ossctl/AGENTS.md`). In practice the release ships a **source path** (`cargo install glasspad`) plus prebuilt binaries + installers (shell + Homebrew tap `jarimustonen/homebrew-glasspad`), covering **macOS arm64** and **Linux arm64 + x86_64**. glasspad's binary matrix is *narrower* than the ossctl canon by two deliberate, glasspad-specific choices, both wired in `dist-workspace.toml`: **(1)** the Linux targets are `gnu`, not statically-linked `musl` (`gnu` is the low-friction default on the GitHub-hosted Ubuntu runners; `musl` would need extra CI toolchain setup for a glibc-independent static binary this project doesn't need — `reqwest` uses `rustls-tls`, so OpenSSL is *not* the reason); **(2)** `x86_64-apple-darwin` and Windows are **not** built as binaries — the macOS release currently uses a personal self-hosted Apple Silicon runner (Homebrew Rust = no rustup, so it cannot cross-compile the Intel-mac target), and Windows binaries are out of scope for this project. **Intel-Mac and Windows users install via the source path (`cargo install glasspad`)** — so those platforms stay covered, just without a prebuilt artifact. The self-hosted-runner routing in `dist-workspace.toml` (`[dist.github-custom-runners]`, which MUST stay at the end of the `[dist]` table) is a **personal / non-standard infra override**, not glasspad canon: other users of the repo will not have that runner. Treat a macOS-only or Linux-only install story as a release gap.

**Merge & review.** Autonomous spinoff units self-merge once their brief's review + adversarial tests pass and no user decision is required; anything genuinely ambiguous is surfaced to the orchestrator, not decided silently. (Interactive `/worktree-code` units, when used, are merged by the user via `/worktree-merge`.)

**Main stays clean.** Commit issue/status/doc changes immediately; never leave `main` modified-but-uncommitted across a session boundary (parallel worktrees branch from `main`'s current state). Pushing and publishing are **pre-authorized** (see **Release autonomy**), gated on green checks rather than a per-step go-ahead — not held for the user.

**CLI surface** follows the AI-first conventions above: strict validation, `--json`, no interactive prompts, informative errors.

## Debugging rendered output

Glasspad is an **HTML-artifact host** (v0.2): the calling agent authors HTML in
a directory and `glasspad serve ./dir` hosts each file in a null-origin
sandboxed iframe. There is no content-DSL and no server-side renderer — the old
`src/spec/*` + `src/client/dashboard.js` path was removed. The host internals
live in **`crates/glasspad-cli/src/artifact_host/`** (see its `AGENTS.md`).

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

**Green-gate gotcha — false `version_cli` failure.** `cargo test` may report a
spurious `commit … got: Null` in `tests/version_cli.rs`: `build.rs` stamps the git
SHA into `option_env!(GLASSPAD_COMMIT)`, but cargo/sccache doesn't re-bake it on an
incremental rebuild when only the build-script SHA changed, so the local binary keeps a
stale/`null` stamp. Fix: `rm -rf target/debug/build/glasspad-* && cargo test`. Clean
CI/release builds never hit this — it is a local-incremental artifact, not a real defect.

Use `./test-browser.sh` for ad-hoc browser automation (requires Brave > View >
Developer > Allow JavaScript from Apple Events). Always check
`./test-browser.sh errors` first.

Full debugging guide: **[AGENTS-GUI-DEBUGGING.md](AGENTS-GUI-DEBUGGING.md)**
