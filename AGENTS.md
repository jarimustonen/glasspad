# Glasspad

Glasspad is an HTML-artifact host for AI agents: an agent writes HTML or markdown
into a directory, `glasspad publish ./dir` returns a loopback or hosted URL, and every
page renders inside a null-origin sandboxed iframe. The whole product promise is that
hostile content cannot escape that sandbox, reach another space, or exfiltrate data.
`README.md` describes the product, `ARCHITECTURE.md` maps the code, `DESIGN.md` is the
visual design system, and `crates/glasspad-cli/src/artifact_host/AGENTS.md` holds the
security contract and its frozen decisions. This file adds what those do not: how the
repository is worked, what is at stake in it, and the traps that the sources do not
reveal.

## How the repository is documented

Every directory that needs agent context has an `AGENTS.md`, with `CLAUDE.md` as a
symlink to it and `AGENTS-<TOPIC>.md` files for topics too large to inline. Durable
knowledge belongs in the most specific of these, not in a personal memory store.

Two directories are gitignored on purpose: `history/` is the agents' scratchpad for
ephemeral notes and review reports, and `.worktree/` holds agent worktree checkouts.

`AGENTS-GUI-DEBUGGING.md` predates the artifact host and still describes the removed
pad server and `dashboard.js`. Its browser-automation mechanics (Brave via osascript,
the isolated-world gotcha, the Vega-Lite axis notes) still apply; its rebuild and
`deploy` workflow does not.

## The CLI surface

Every command follows the family's AI-first CLI canon: strict input validation,
`--json` envelopes, JSONL logs, no interactive prompts, informative errors, composable
commands. The canon is the `/ai-first-cli-canon` skill shipped by `project-canon`.
There is deliberately no repo-local copy: an earlier copy drifted from the canon, so
changes to the canon go to the `project-canon` source and are reinstalled from the
released tool.

## Issues and planning

Work is tracked with `issuectl` under `issues/`, one descriptive kebab-case slug per
issue, status in frontmatter. The `/issue` skill documents the commands and the JSON
contract; `.issuectl/AGENTS.md` carries the tracker's own agent policy. Slugs are
words, never numbers, because agents and humans grep for them by meaning.

Planning documents (plans, analyses, designs, todos) live under the issue they serve,
as `issues/<slug>/plan.md`, `analysis.md`, or `design.md`, never as standalone files.
The reason is traceability: closed issues keep their plans, so `ARCHITECTURE.md` can
send a reader to an issue for the *why* behind a decision. If a piece of work needs a
plan, it needs an issue first.

`TODO.md` at the repo root is the round-by-round handoff for `/stint` sessions: where
things stand, what to start on, and the standing lessons earlier rounds paid for. It is
orientation only; `issuectl dag` is authoritative for scheduling. Read its standing
lessons before writing a worker brief, since several of them exist because a brief
once omitted them.

## How work moves

A `/stint` session is the orchestrator the user talks to in product-owner language. It
plans rounds, triages incoming bugs, reports status, and owns the local deploy. It does
not write code itself: coding happens in worktrees spawned via the `/worktree` family,
so that units run in parallel and `main` is never half-edited under them. Autonomous
spinoffs (`/worktree-spinoff --headless`) are the default and self-merge once their
brief's review and adversarial tests pass; interactive `/worktree-code` units are for
work the user wants to review, and the user merges those with `/worktree-merge`. A unit
that touches production or security code gets `/llm-review` plus `/assess-findings` in
its brief.

Parallel worktrees branch from whatever `main` currently is, so `main` stays committed.
Commit issue, status, and doc changes as you make them, and never leave `main` modified
but uncommitted across a session boundary.

**On interrupting the user.** These sessions run long and largely unattended, and the
user's attention is the scarcest resource in the system. A question is worth asking
when the outcomes differ in a way the user would care about and you cannot tell which
they would choose: a genuine fork where reasonable people disagree, something that
cannot be done, or the fix / defer / not-a-bug call on a reported bug, which is always
the user's. Everything routine is yours: spawning worktrees, merging landed units,
deploying to localhost, starting the next unit, syncing `main` with `origin` (pull
with rebase, resolve, push), and cutting releases. Make the choice, say what you chose,
keep moving. The user has said, on three separate occasions, that being asked "shall I
cut the release?" is a cost, not a courtesy; "continue developing" does not suspend any
of this.

Anything in a worktree that only reads, builds, or tests is free to run: `cargo build`,
`cargo test`, `cargo clippy`, the security suite, browser automation. Verifying a change
end to end before it merges is the expected standard, not a nicety.

## Releasing

A release is a technical gate, not a permission. When a releasable change is on `main`
and the gate is green, cutting the release is part of moving work forward, and the
decision is yours. Report after the fact.

The gate is wider than CI's. All of these pass before a tag is pushed:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo publish --dry-run
./test-security.sh
shipshape audit --json      # readiness core must report complete
```

The version to cut is the one in `Cargo.toml`; bumping it is not a separate user
decision. The bundled skill's `cli_version` in `crates/glasspad-cli/src/skill.md` has
to match, and a test pins that, so run the gate again after both edits. Check the
`[Unreleased]` section of `CHANGELOG.md` against what actually landed before finalizing
it: units have written their line into an already-published section when a release was
cut while their branch was alive.

**Publishing happens in CI, triggered by the tag push alone.** Pushing a
`vMAJOR.MINOR.PATCH` tag runs `publish-crates.yml` (crates.io, via the
`CARGO_REGISTRY_TOKEN` repo secret) and cargo-dist's `release.yml` (binaries, the GitHub
Release, the Homebrew formula) in parallel. So "cut a release" is: land the change,
push `main`, tag, push the tag. Nothing else. Do not run a real local `cargo publish`
(the local `~/.cargo/credentials.toml` may be stale, and a 403 from it says nothing
about the release path) and do not `gh release create` (cargo-dist owns Release
creation; a second one collides). Shipshape knows this boundary and journals the
publish steps rather than running them. A crates.io version is permanent and a pushed
tag is proxy-cached, which is why the dry run and the full gate come first and why a
failed CI leg is retried with "Re-run failed jobs" on the original run, never by
deleting and re-pushing the tag. The rest of the release contract, including why each
channel is declared the way it is, is in `OSS-RELEASE.md`.

The only reasons to stop short of a release and surface it: a gate fails and the fix is
a genuine fork; the release tooling on this machine has not converged (say so; do not
skip or substitute a gate); or the CI-side crates.io credential is genuinely absent.

**Cross-platform is a hard requirement.** glasspad installs and runs on macOS and
Linux; a release path that covers only one is incomplete. The binary matrix in
`dist-workspace.toml` is deliberately narrower than the family's Shipshape canon, and
the comments there explain each choice (gnu rather than musl on Linux; no Intel-Mac or
Windows binaries, those users install with `cargo install glasspad`). Two things about
that file are easy to break. The macOS build is routed to a personal self-hosted Apple
Silicon runner under `[dist.github-custom-runners]`; that is the operator's
infrastructure, not project canon, and other users of the repo will not have it. That
sub-table has to stay last in the file, because in TOML any key written after its
header belongs to the sub-table, not to `[dist]`. And `release.yml` is generated from
`dist-workspace.toml` with an approved override applied by `scripts/release_workflow.py`;
edit the config and regenerate rather than hand-editing the workflow.

## Crate layout

`ARCHITECTURE.md` has the map. Three decisions behind it are not obvious from the code
and have been challenged before:

`crates/glasspad-core` is pure: no `clap`, no `std::fs`, no network, no
`SystemTime::now()`. Time enters through the `Clock` trait in `time.rs`, with the
wall-clock implementation at the CLI edge, so domain decisions are testable without a
clock. When you touch core, verify purity by grep rather than by reading.

The two crate roots are one published package on purpose. `Cargo.toml` points `[lib]`
and `[[bin]]` at the two roots so that `cargo publish`, `cargo install glasspad`,
cargo-dist, and the tag-triggered CI stay single-package operations. Splitting into two
published crates would break the source-install path that Intel-Mac and Windows users
depend on. A CLI-canon conformance report that calls the incomplete split a shortfall
is correct and accepted; it is not a defect to fix.

`hosted` stays in the CLI crate. It is a durable on-disk store plus an HTTP surface
with a few hundred filesystem and network touchpoints; it *is* the shell the canon's
library-first section means. A canon audit that flags it as an unmoved domain module
should be rejected. The pure parts worth extracting (rendering, sanitization, shell and
template output, CSP policy) already live in core.

## Working on the host

After changing host code or a base library under `/_gp/v1/` (`base.css`, `charts.js`,
`bridge.js`, `manifest.json`), the assets are compiled into the binary: rebuild,
restart the loopback server, reload the space.

`./test-security.sh` is the regression gate for the product promise. It builds, boots a
loopback server on a test port with its own pid file and state directory (so it does not
disturb a running local deploy), drives headless Chromium through the adversarial
probes, then runs the server-side Wave 2a probes. Run it after any change to the host,
headers, CSP, shell, or bridge. Green means the run reached its final "Wave 2a
space-model probes PASSED" line and exited 0. The script is `set -e`, so the first
failing probe aborts mid-suite, and a worker can truthfully report "all Phase 1 checks
passed" while Wave 2a never ran; a release was halted once for exactly that. The check
count drifts as probes are added, so trust the script's own summary, not a number
written in a document. A leftover `glasspad` serve process from an earlier run causes
a spurious early death.

`cargo test` can fail `tests/version_cli.rs` with `commit … got: Null` on a local
incremental build. `build.rs` stamps the git SHA into the binary, and cargo does not
re-run it when only the SHA changed, so the binary keeps a stale stamp. It is not a
defect; clean CI and release builds never see it. Clear it with:

```bash
rm -rf target/debug/build/glasspad-* && cargo test
```

`./test-browser.sh` drives the user's Brave browser through osascript for ad-hoc DOM
checks, so it is macOS-only and needs Brave's "Allow JavaScript from Apple Events"
enabled. Run its `errors` command first: the page shows chart errors visibly, and
automated checks have missed errors that were sitting on screen.
