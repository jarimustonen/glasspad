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
symlink to it (the issuectl-managed `issues/AGENTS.md` and `.issuectl/AGENTS.md` have
none) and `AGENTS-<TOPIC>.md` files for topics too large to inline. Durable
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
commands. The canon is the `/ai-first-cli-canon` skill shipped by `project-canon`. The
repo-local copies under `.claude/skills/ai-first-cli-canon/` and `.codex/prompts/` are
written by `project-canon skill install`, never by hand: an earlier hand-kept
`AGENTS-AI-FIRST-CLI.md` copy drifted from the canon, so changes to the canon go to the
`project-canon` source and are reinstalled from the released tool.

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

The session narrative (where a round stands, what to start on, open decisions) is kept
outside this repository; the `issuectl dag` is the schedule. Standing rules live in the
`AGENTS.md` files, and the ones below under "Writing a worker brief" exist because a
brief once omitted them.

When ranking triaged work, rank by how often the effect happens, not by how bad its
worst case sounds: something that bites on every publish outweighs data loss under a
rare crash, because the frequent cost is the one users actually pay.

## How work moves

A `/stint-start` session is the orchestrator the user talks to in product-owner
language. It plans rounds, triages incoming bugs, reports status, and owns the local
deploy. It does not write code itself: coding happens in worktrees spawned via the
`/worktree` family, so that units run in parallel and `main` is never half-edited under
them. Autonomous spinoffs (`/worktree-spinoff --headless`) are the default and
self-merge once their brief's review and adversarial tests pass; interactive units
(`taskfleet run create --kind spinoff --interactive`) are for work the user wants to
review, and the user finalizes those with `taskfleet run merge` or `run cancel`. A unit
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

**Writing a worker brief.**

- Put a `CHANGELOG.md` line in the done criteria, and name the section it goes in
  (normally `[Unreleased]`). Without the first, units land with no entry; without the
  second, a unit whose branch outlives a release cut writes into the already-published
  section and the changelog claims the feature shipped a release early.
- A brief that includes `/llm-review` also carries the speculative-findings filter from
  `.issuectl/AGENTS.md`; otherwise each review round regenerates findings that are not
  work.
- State a constraint only for behaviour you have confirmed in the code or by running
  the binary. A worker follows a false premise faithfully, and can add a refusal or
  check that never existed, silently changing production behaviour.
- When a unit lands, smoke-test its claim yourself rather than trusting its summary,
  and re-check a "security green" claim against the full `./test-security.sh` run
  (see below): partial runs have been reported as green.

**Public means no personal setup.** The repository is public, so commits, issues,
docs, tests and fixtures name no personal machine names, personal filesystem paths,
private sibling repositories, internal URLs, or personal contact addresses. The
maintainer's public account is not in that class: `github.com/jarimustonen/glasspad` in
an install command is fine. Old history needs no scrubbing for its own sake.

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

Release finished user-visible work before a large restructure (a crate split, anything
touching `Cargo.toml`, `dist-workspace.toml`, or the published package shape), not
after. A broken pipeline only surfaces at tag time, and releasing first keeps a
proven-good pipeline and keeps finished work from waiting behind the debugging.

**Cross-platform is a hard requirement.** glasspad installs and runs on macOS and
Linux; a release path that covers only one is incomplete. The binary matrix in
`dist-workspace.toml` is deliberately narrower than the family's Shipshape canon, and
the comments there explain each choice (gnu rather than musl on Linux; no Intel-Mac or
Windows binaries, those users install with `cargo install glasspad`). Two things about
that file are easy to break. The macOS build is routed to a personal self-hosted Apple
Silicon runner under `[dist.github-custom-runners]`; that is the operator's
infrastructure, not project canon, and other users of the repo will not have it; its
runner environment must not set `GIT_CONFIG_GLOBAL`, which breaks the macOS build. That
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

The hosted store's single mutation lock is deliberately coarse. Narrowing it (staging
and fsync outside the lock, holding it only for the pointer flip) pays off only under
concurrent publishes, and there is one publisher; it would also break GC's invariant
that any dotted staging directory is a crash remnant, and the snapshot
read-modify-write. Do not file or implement it unless there is evidence: more than one
API key in use, or measured publish latency showing GC contention.

## Working on the host

The public hosted service runs on haapa as the `glasspad-host.service` user unit.
Jari has authorised build and deploy when there is something to build or deploy;
do not restart it merely as a ritual. Follow the owning Homebase policy in
`~/Sources/homebase/infra/glasspad/AGENTS.md`: release binaries, never install a
local source build over the shared CLI. When the installed release is newer than
the running process, `systemctl --user restart glasspad-host.service` on haapa
activates it; when the installed release is old, the owning local update command
is `cd ~/Sources/homebase && ./infra/glasspad/refresh-server.sh` (checksum-verified,
health-checked, with rollback). Check the running process, not only `glasspad
version --json`: the fleet installer may replace its executable while the old
process remains alive. Verify `http://127.0.0.1:18795/healthz` and
`https://glasspad.maalla.dev/healthz` after a restart. Do not touch the page
store, API keys, or shared Cloudflare tunnel as part of a version update.

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

The dependency tree (`idna_adapter` → `icu_*`) needs rustc 1.86 or newer. If a plain
`cargo` fails on a minimum-version error, the machine's default toolchain is pinned
older; run the gate under `rustup run stable` rather than downgrading dependencies.

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
