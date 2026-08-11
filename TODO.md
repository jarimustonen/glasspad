# TODO — Glasspad handoff

Orchestrator entry point for `/stint`. This is the round-by-round plan; the
authoritative detail lives in the issue tracker (`issuectl`), not here.

## Where we are

**glasspad 0.4.0 is FULLY RELEASED** (2026-08-10) — crates.io `0.4.0`, GitHub
Release `v0.4.0` (12 assets: mac `aarch64-apple-darwin` + 2× Linux tarballs,
`installer.sh`, `glasspad.rb`), Homebrew tap `homebrew-glasspad` (`version "0.4.0"`),
all live and verified. `main` carries 0.4.0 (`Cargo.toml`) + tag `v0.4.0` (commit
`b6dad4c`). This shipped the **artifact return channel** (`gp.submit` /
`await-submission`) that had landed on main since 0.3.1. Prior baselines: 0.3.1
(2026-08-10, hosted inter-page link fix), 0.3.0 (2026-08-09, agent→HTML consolidation).

**The release was a single clean tag push** (`git push origin v0.4.0`), both
workflows green off the tag; **the macOS build ran on `hauis` in ~1m and passed** —
first tag-triggered release since the 2026-08-09 durable gitconfig fix, which held
clean (no re-runs, no manual intervention).

The 0.3.0 GitHub-Release completion (blocked 2026-08-06→09 by hauis's mac job) was
finished 2026-08-09 via the same **durable runner-gitconfig fix** — see below.

**Round 2026-08-09/10 (this session) — 5 units landed on main, full green gate
(fmt/clippy/test + `./test-security.sh`, now 48 checks + Wave 2a), issue tracker now empty:**
- ✅ `hosted-config-path-macos` — `publish` honors `$XDG_CONFIG_HOME`/`~/.config` on all
  platforms (matches `--help`); old `dirs::config_dir()` path still read as fallback.
- ✅ `hosted-noindex-missing` — `X-Robots-Tag: noindex, nofollow` on hosted read routes
  (host-serve only; loopback `serve` untouched) + regression test.
- ✅ `particularly-offbeat-dust` — optional `idempotency_key` on `POST /api/v1/pages`
  (per-tenant scoped, fsync + atomic mapping; no key → today's behaviour byte-for-byte).
- ✅ `mac-release-self-hosted` — **reverted** the mac release build `macos-14` →
  self-hosted `hauis`, now that hauis is durably fixed and is the intended mac machine.
- ✅ `artifact-return-channel` — interactive artifacts return user input to the creating
  agent via `gp.submit()` → trusted-shell airlock → server → `glasspad await-submission`
  (backgrounded server-side long-poll). Hosted + loopback; artifact sandbox stayed frozen
  (regression-asserted); `/llm-review` + 7 new security-Wave cases. **Design + decision
  docs:** `issues/artifact-return-channel/{design,models-comparison}.md`.

**0.3.0 features landed this session (all green: fmt/clippy/test + `./test-security.sh`
41 + Wave 2a; each had a multi-model `/llm-review`):**
- ✅ `markdown-template-render` — `glasspad render <file.md> [--template …]`, server-side
  md+template into the sandbox wrap seam (`src/artifact_host/render.rs`).
- ✅ `hosted-share-server` — hosted run mode + `glasspad publish`: API-key ingest,
  128-bit capability-slug public URLs (`/p/<slug>`), retention/GC, multi-tenant
  (`src/hosted/`). Loopback `serve` + its DNS-rebinding guard untouched.
- ✅ `static-build-output` — `glasspad build <space> <out>` static self-contained render.
- ✅ `serve-process-mgmt` — `glasspad stop`, `GLASSPAD_PORT`, PID file.
- ✅ `version-commit-stamp` — real git SHA in `version --json`.
- ✅ `skill-routing-guidance` — serve vs render vs publish vs build guidance in `src/skill.md`.
- (Earlier: `prose-theme`, `version-command`.)
- ✅ `release-mac-github-runner` — mac release build was moved off `hauis` → GitHub-hosted
  `macos-14` (2026-08-07), then **reverted back to self-hosted `hauis` 2026-08-09**
  (`mac-release-self-hosted`) once hauis was durably fixed. Mac builds run on hauis again.

## ✅ 0.3.0 GitHub Release — DONE (2026-08-09), via durable hauis fix

Chose option (a): fix hauis + rerun `31112313027 --failed`, keeping the published
`v0.3.0` tag (agent never touched the tag). The mac build ran **on hauis** and the
Release + Homebrew jobs completed green.

**Root cause (recurred multiple times) & durable fix** — the mac job failed because
of the runner-gitconfig setup, NOT a one-off:
- hauis's `~/.gitconfig` was a symlink → tracked `dotfiles/src/.gitconfig`, so
  `actions/checkout` + cargo git ops polluted the tracked file. The 2026-08-08 fix
  set `GIT_CONFIG_GLOBAL=~/.gitconfig-actions` per runner — which kept dotfiles clean
  but **broke checkout auth** (checkout needs `--global == $HOME/.gitconfig`; the
  override redirected the write → "Unable to replace auth placeholder"; earlier, with a
  stale extraheader in that file, a duplicate-auth `HTTP 400`).
- **Durable fix (2026-08-09):** `~/.gitconfig` is now a real file that `[include]`s the
  dotfiles config. Git reads the include but never writes into it → debris stays in
  top-level `~/.gitconfig`, dotfiles stays clean, AND checkout's `--global==$HOME`
  invariant holds. `GIT_CONFIG_GLOBAL` removed from all 4 runner `.env` files; runners
  restarted. **Full write-up committed to `homebase/infra/machines/hauis.md`.** Do NOT
  re-add `GIT_CONFIG_GLOBAL` — that is the thing that breaks mac builds.

## Round 2026-08-11 — return-channel A2 + B2 landed (both green, reviewed)

Two Lane-B units landed on `main` (`b1ef061`, pushed to origin), sequenced because they
share `src/hosted/*` + `src/cli.rs` + `src/server.rs`. Full round green gate passed:
`cargo fmt --check` + `clippy -D warnings` + `cargo test` + `./test-security.sh` (Wave 2a
✅ + new B2 round-push probes + new A2 SSE-streaming probes, all PASS). **Not yet
released** — code is on `main` at 0.4.0; a release would be 0.5.0 (see below).
- ✅ `return-channel-multi-round` (B2) — multi-round: after `gp.submit()` the agent
  re-renders the artifact in place via an owner round-push over the shell's live-reload
  SSE. New `src/hosted/rounds.rs`; round binding rejects stale-round submits (409);
  each round stays null-origin `connect-src 'none'` (airlock held, regression-asserted).
  `/llm-review` + fixes applied. Issue `done`.
- ✅ `return-channel-sse` (A2) — SSE transport for `await-submission`:
  `GET /api/v1/pages/<slug>/submissions/stream`, `since=<id>` cursor, per-tenant
  isolation (cross-tenant → opaque 404), live push during hold; long-poll stays default.
  Added `reqwest` `stream` feature. `/llm-review` + fixes (correctness + DoS). Issue `done`.
- 🌱 A2 worker filed a new backlog feature: **`multipage-hosted-space`** (multi-page
  hosted publish / space ingest + markdown-native spaces — a tilictl docsite use case).
  Open, unstarted, needs scoping.

## ▶ Start here (on return)

**Nothing in flight.** `main == origin` (`b1ef061`), clean tree, 0.4.0 in `Cargo.toml`.
The return channel now has both later increments (A2 SSE + B2 multi-round) on `main`,
green + reviewed, **unreleased**. Two open decisions for the next round:
1. **Cut 0.5.0?** A2+B2 are a releasable, meaningful feature bump on top of shipped
   0.4.0. Release autonomy applies (green gate + tag-push→CI recipe). No hard gate —
   the agent may decide to cut it.
2. **`multipage-hosted-space`** — the only open issue; needs scoping/decompose before a
   worktree (Lane B, hosted core). Pick it, or defer.

**Candidate next work (no hard gate, pick or defer):**
- **Return-channel A2/B2 increment** — A2 (SSE transport) / B2 (multi-round); the
  versioned submission record already leaves room (`issues/artifact-return-channel/models-comparison.md`).
- **Agent-facing skill doc** — document the `gp.submit` → `await-submission` round-trip
  in the skill guidance if not already covered.
- **Optional polish** (below): cosmetic LICENSE/contact confirms, close `release-oss`
  epic, `version-commit-stamp` provenance follow-up.
- **Downstream** homebase + tilictl consolidation, gated on the shipped glasspad (tracked
  in those repos).

_Released this round (2026-08-10):_ **0.4.0** — cut end-to-end with standing release
autonomy on a green gate (fmt + clippy -D warnings + test + `cargo publish --dry-run` +
`./test-security.sh` 48 + Wave 2a). CHANGELOG `[Unreleased]` had been left empty when the
feature landed; this round wrote the return-channel entry as `[0.4.0]`. The generic
`ossctl release cut` engine was deliberately **not** used — its `publish-all` phase does a
*local* `cargo publish`, which CLAUDE.md forbids (CI-side token only); the repo's
tag-push→CI recipe was used instead (as for 0.3.0/0.3.1).

### Optional polish (no hard gate)

- **Cosmetic confirms:** LICENSE holder "Jari Mustonen"; SECURITY.md / CoC contact
  `jari@itsellesi.fi`. Change via normal edit + patch release if wanted.
- **Close the `release-oss` epic** — effectively complete.
- `version-commit-stamp` follow-up (recorded in `history/assessment-version-commit-stamp.md`):
  read `.cargo_vcs_info.json` for crates.io-tarball provenance. Low.
- Next forward work: downstream homebase + tilictl consolidation, gated on 0.3.0 (tracked there).

## Execution DAG (2026-08-11)

Scheduling PLAN — source of truth for lane + order; issuectl is authoritative for STATUS
(never copied here). Merge each round (drop landed, add active, keep existing order).
`▶` = head-of-line snapshot — RE-COMPUTE from issuectl at pick time.
`after <slug> (needs …)` = logical blocked_by mirror. `collision: <file>` = touches a
second lane's hot file (spawn-time exclusion).

Hot files → lanes: `src/artifact_host/assets/base.css` (design system, Lane A);
`src/cli.rs` + `src/server.rs` + render modules (Lane B). `src/skill.md` is docs-only.

<!-- execution-dag:begin -->
```
GLOBAL HEAD-OF-LINE: pidev-dual-home-skills   ← urgent (high); start here

LANE B — src/cli.rs + src/main.rs + src/hosted/* (CLI dispatch + hosted core)
  ▶ pidev-dual-home-skills   (high — glasspad skill install dual-homes into ~/.pi/agent/skills; touches cli.rs/main.rs)
    multipage-hosted-space   collision: src/cli.rs + src/main.rs (after pidev; needs scoping/decompose)
```
<!-- execution-dag:end -->

## How to cut a release (the recipe, now automated)

`git push origin vX.Y.Z` triggers **both** workflows off the tag:
- `release.yml` (cargo-dist) → builds binaries + pushes the Homebrew formula to the tap.
- `publish-crates.yml` → `cargo publish` to crates.io.

Bump the version in `Cargo.toml`, add a `CHANGELOG.md` entry, commit, then tag+push.
**Caveats learned this stint:**
- **The macOS build runs on the self-hosted `hauis` runner** (`dist-workspace.toml`:
  `aarch64-apple-darwin = "self-hosted"`). Hauis's runner-gitconfig was durably fixed
  2026-08-09 (`~/.gitconfig` `[include]` split replacing the broken `GIT_CONFIG_GLOBAL`
  override; write-up in `homebase/infra/machines/hauis.md`). **Do NOT re-add
  `GIT_CONFIG_GLOBAL` to any runner `.env`** — that is what broke mac builds. (Interlude:
  2026-08-07→09 the mac build was briefly routed to GitHub-hosted `macos-14`, then reverted.)
- **Cut a tag ONCE and let it finish** — still good hygiene; don't re-tag mid-run.
- **`release.yml` has no `workflow_dispatch`** — a failed release can only be re-run
  (`gh run rerun <id> --failed`) or re-triggered by re-pointing the tag; there is no
  manual dispatch path.
- Secrets already set on the repo: `CARGO_REGISTRY_TOKEN`, `HOMEBREW_TAP_TOKEN`.
- The agent has **standing release autonomy** (see `AGENTS.md` → Operating Policy):
  may cut + publish releases autonomously, gated on green checks, including deciding
  to release. Pushing/publishing is pre-authorized.

## Backlog

- **0.3.0 agent→HTML consolidation — FULLY RELEASED 2026-08-09** (crates.io + GitHub
  Release + Homebrew). All feature/infra issues landed; GH-Release completion done.
- **2026-08-09 round — all four landed + green:** `hosted-config-path-macos` (fixed —
  publish now honors `$XDG_CONFIG_HOME`/`~/.config` on all platforms, old
  `dirs::config_dir()` path still read as fallback), `hosted-noindex-missing` (fixed —
  `X-Robots-Tag: noindex, nofollow` on hosted read routes, host-serve only + regression
  test), `particularly-offbeat-dust` (done — optional `idempotency_key` on
  `POST /api/v1/pages`, per-tenant scoped, fsync+atomic mapping), `mac-release-self-hosted`
  (done — mac build reverted `macos-14` → self-hosted hauis).
- **Optional follow-up (low, not filed):** `version --json`'s commit stamp uses
  `option_env!(GLASSPAD_COMMIT)`, which cargo/sccache doesn't re-bake on incremental
  local rebuilds when only the build-script SHA changes — a stale/`null` local stamp
  until a clean rebuild. Clean CI/release builds are always correct, so this is a
  dev-ergonomics nit, not a shipped defect. File only if it annoys in practice.
- **`artifact-return-channel` — RELEASED in 0.4.0 (2026-08-10)** (crates.io + GitHub
  Release + Homebrew, all live). New public API surface — `gp.submit`, submit/poll/wait
  endpoints, `glasspad await-submission` — shipped via the tag-push→CI recipe; mac built on
  hauis. Now demoable on deployed maalla.dev.
- **Later increment for the return channel:** A2 (SSE transport) / B2 (multi-round) — the
  versioned submission record already leaves room; see `models-comparison.md`.
- Downstream homebase + tilictl consolidation is the next forward work, gated on 0.3.0
  (tracked in those repos).

## Verify / deploy (localhost)

Per `CLAUDE.md`: after editing host code or a base lib, `cargo build`, restart
`glasspad serve`, reload the space. `./test-security.sh` (48 browser checks +
Wave 2a probes) is the regression gate after any host/header/CSP/bridge change.
Note: the `version --json` commit-stamp test can false-fail on an incremental local
build — `rm -rf target/debug/build/glasspad-*` then re-run (see root `AGENTS.md`).
Use `./test-browser.sh` (check `./test-browser.sh errors` first) for ad-hoc
browser automation.

## Notes for the orchestrator

- Keep `main` clean — commit issue/status changes immediately (`CLAUDE.md`).
- The repo is public now; treat commits/history as public.
- `/oss-*` skills (over `ossctl`) drive release/readiness work; `ossctl audit` scores gaps.
- Track all planning under the issue, not as loose files.
