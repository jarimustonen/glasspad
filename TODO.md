# TODO — Glasspad handoff

Orchestrator entry point for `/stint`. This is the round-by-round plan; the
authoritative detail lives in the issue tracker (`issuectl`), not here.

## Where we are

**CURRENT: glasspad 0.7.0 is FULLY RELEASED + verified live** (2026-08-13) — crates.io
`0.7.0`, GitHub Release `v0.7.0` (12 assets), Homebrew `version "0.7.0"`. Ships the
**publish-first CLI surface** (`publish` = default verb, config-driven loopback|hosted)
+ **emoji SVG favicon**. Details in _Round 2026-08-13_ below; next up is `space-docsite-nav`
(see _▶ Start here_). The 0.3.0→0.6.0 release history is preserved below for context.

**glasspad 0.4.0 was FULLY RELEASED** (2026-08-10) — crates.io `0.4.0`, GitHub
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

## Two releases cut 2026-08-11 — 0.5.0 then 0.6.0 (both autonomous)

Both cut **autonomously** per reaffirmed release autonomy (no permission asked; a green
gate is the go). Each: fmt + clippy -D warnings + cargo test + `./test-security.sh`
(48 + Wave 2a) + `ossctl audit` (core complete, 0 gaps) + `cargo publish --dry-run`,
then tag-push → both CI workflows (Release/cargo-dist + Publish-to-crates), mac on hauis.

- **0.5.0** (`1763964`, tag `v0.5.0`) — return-channel **A2 (SSE)** + **B2 (multi-round)**
  + **pi.dev dual-home skill install**. Verified live on all three channels (crates.io
  0.5.0 via sparse index, GitHub Release v0.5.0 12 assets, Homebrew `version "0.5.0"`).
- **0.6.0** (`aa4005f`, tag `v0.6.0`) — **multi-page hosted docsite**: Gap 1
  `publish-space` (space ingest → `/{space}/…`, nav + relative links, idempotent slug,
  cross-tenant 404) + Gap 2 markdown-native spaces (serve/build/publish-space render a
  dir of `.md`, slug = stem). 389 tests + 143 security PASS. **CI green — verified live on
  all three channels** (crates.io `0.6.0` yanked=false, GitHub Release `v0.6.0` 12 assets,
  Homebrew `version "0.6.0"`); mac build ran clean on hauis (2nd clean tag-release of the day).

## Design phase 2026-08-12 — publish-first CLI surface (3 issues filed, design DONE)

0.6.0 is released + verified live (all three channels). This session was a **design
phase** — no code landed; Jari and the orchestrator co-designed a big CLI reshape and
filed it. `main == origin` (`852163f`), clean tree, still 0.6.0 in `Cargo.toml`.

The motivating problem: the CLI + skill push agents to loopback `serve`/`open` by default
(the skill literally says "default to loopback serve"), but the intended standard flow is
**hand glasspad markdown → get a hosted URL**. The home config already points at
`glasspad.maalla.dev` but is treated only as a publish credential source, not as "hosted
is the default".

**Filed (all decisions locked — see `issues/publish-first-surface/design.md`, published
at a hosted URL this session):**
- **`publish-first-surface`** (high, design-first) — make **`publish` THE default verb**;
  resolve `target: loopback | hosted` from config precedence `.glasspad.yaml` (repo) →
  `~/.config/glasspad/config.yaml` (home) → built-in default (loopback), **merged
  per-key**. Merge `publish`+`publish-space` (a file = a 1-page space). **Remove**
  `serve`/`create`/`render`/`open` (NO back-compat); demote `build` to advanced
  (raw-HTML/debug); loopback mgmt regrouped under **`glasspad loopback <cmd>`** (advanced,
  help-only). Markdown-first. Skill.md rewrite is part of it. Hosted = snapshot +
  idempotent re-publish (live-reload stays a loopback property).
- **`emoji-favicon`** (normal) — zero-dep emoji **SVG** favicon (`<svg><text>…</text>`) for
  published + built pages; emoji from the repo's `.glasspad.yaml` (`favicon: 🚀`), default
  fallback. After the config unit of publish-first-surface (shares `.glasspad.yaml`).
- **`hosted-multiworker-credentials`** (low, **deferred**) — FUTURE: secure credential
  model for many workers (per-worker scoped tokens, rotatable, secret-manager/env source,
  not a shared plaintext home key). Constraint on publish-first: `.glasspad.yaml` `api_key`
  must accept an **indirection** (env/key-file/secret ref), not only an inline secret, so
  this layers on later without a schema break.

## Round 2026-08-13 — 0.7.0 SHIPPED (publish-first surface + emoji favicon)

The publish-first design (2026-08-12) was **built, reviewed, released, and verified live**
this session. Two Lane-B spinoffs landed on `main`, sequenced (both touch cli.rs/config):
- ✅ `publish-first-surface` (`7d6d60e`+`6ac5307`) — **`publish` is THE default verb**
  (markdown-first; file = 1-page space, dir = N-page space); config-driven `target:
  loopback|hosted` via new `src/config.rs` per-key merge (`.glasspad.yaml` → home →
  loopback default). `api_key` accepts an env/file **indirection** (room for
  `hosted-multiworker-credentials`). `serve`/`create`/`render`/`open`/`stop` **removed**
  as top-level verbs (regrouped under `glasspad loopback <serve|open|stop>`); `build` kept
  advanced. `src/skill.md` rewritten around "hand glasspad markdown, get a URL." `/llm-review`
  (4 models) + fixes. Issue `done`.
- ✅ `emoji-favicon` (`344764e`+`9c18c78`) — zero-dep inline **SVG emoji favicon** on the
  OUTER served/built document (new `src/favicon.rs`: strict validate + XML-escape + base64
  data: URI); emoji from `.glasspad.yaml` `favicon:` else default 📊. Sandbox byte-for-byte
  unaffected (asserted). `/llm-review` (4 models) + fixes. Issue `done`.

**0.7.0 FULLY RELEASED + verified** (`21a65f9`, tag `v0.7.0`, autonomous per release
autonomy). Full green gate: fmt + clippy -D + `cargo test` (288 core + suites) +
`./test-security.sh` (48 + Wave 2a, **security contract untouched**) + `ossctl audit`
(core complete) + `cargo publish --dry-run`. Single tag push → both CI workflows green,
mac built clean on hauis. Live on all three channels: **crates.io `0.7.0`** (sparse index,
yanked=false; the `/api` JSON endpoint cache-lags — index is authoritative), **GitHub
Release `v0.7.0`** (12 assets, not draft), **Homebrew `version "0.7.0"`**. Crate
description updated (`loopback-only` → publish-first). This is the 3rd clean tag-release.

## Cross-repo finding 2026-08-13 — why aggountant still needs `tw view` (filed `space-docsite-nav`)

Investigated why `../aggountant`'s `design-v2` docsite (index-type structure) doesn't port
onto glasspad. Two separate reasons, verified empirically against glasspad 0.7.0:
1. **Loopback-only viewing** → `tw view` bridges server→seat. **0.7.0 hosted `target`
   removes this** once `glasspad.maalla.dev` is upgraded (blocked in aggountant
   `docsite-glasspad-maalla-hosted` — an **ops redeploy**, not a glasspad code gap).
2. **The index/nav structure doesn't port** — glasspad's space model is structurally flat:
   `nav` is a single-level slug list; with no `index.md` `build` emits a **redirect stub**
   (not a curated landing); dotted companion stems (`*.arkkitehdille.md`) are **rejected**
   as invalid slugs. build_docs.py generates a grouped/nested sidebar + rich landing index
   with per-doc descriptions. **This gap is now filed as glasspad `space-docsite-nav`** (see
   below) — the head of the DAG.

## ▶ Start here (on return)

`main == origin`, clean tree, nothing in flight, 0.7.0 released + verified live. Tracker
holds one active scheduled unit + one deferred backlog item.

**Head-of-line: `space-docsite-nav`** (normal, **design-first**) — grouped/nested space
nav + generated landing index so a structured docsite (aggountant's design-v2 shape:
grouped spec/ADRs/stints + arkkitehdille/kirjanpitajalle companions) ports onto glasspad
without a bespoke `build_docs.py` index/sidebar. Scope + empirical findings + what's
OUT-of-scope (glossary autolink / section-TOC / companion discovery / SVG diagrams stay an
aggountant-side preprocessor) are in `issues/space-docsite-nav/item.md`. Lands on the space
core (`src/artifact_host/space.rs` + manifest + `bridge.js` + render seam) → **Lane B**,
design-first, `/llm-review`, keep `./test-security.sh` green (should be untouched). A
release after it lands would be **0.8.0** (agent decides — release autonomy).

_Older candidates still valid:_ tilictl docsite migration (tracked in tilictl); optional
polish (below).

### Optional polish (no hard gate)

- **Cosmetic confirms:** LICENSE holder "Jari Mustonen"; SECURITY.md / CoC contact
  `jari@itsellesi.fi`. Change via normal edit + patch release if wanted.
- **Close the `release-oss` epic** — effectively complete.
- `version-commit-stamp` follow-up (recorded in `history/assessment-version-commit-stamp.md`):
  read `.cargo_vcs_info.json` for crates.io-tarball provenance. Low.
- Next forward work: downstream homebase + tilictl consolidation, gated on 0.3.0 (tracked there).

## Execution DAG (2026-08-13)

Scheduling PLAN — source of truth for lane + order; issuectl is authoritative for STATUS
(never copied here). Merge each round (drop landed, add active, keep existing order).
`▶` = head-of-line snapshot — RE-COMPUTE from issuectl at pick time.
`after <slug> (needs …)` = logical blocked_by mirror. `collision: <file>` = touches a
second lane's hot file (spawn-time exclusion).

Hot files → lanes: `src/artifact_host/assets/base.css` (design system, Lane A);
**Lane B (server/CLI/hosted core)** = `src/cli.rs` + `src/main.rs` + `src/server.rs` +
`src/submissions.rs` + `src/hosted/*` + `src/artifact_host/space.rs` +
`src/artifact_host/render.rs`. `src/skill.md` is docs-only. (Lane B widened after the
2026-08-11/12 stint: return-channel A2/B2 and the space-ingest/markdown-space work all
collided across this whole family — treat any two units touching it as sequenced, not
parallel.)

<!-- execution-dag:begin -->
```
GLOBAL HEAD-OF-LINE: (none auto-spawnable — markdown-diagrams is next in line but awaits Jari's go; the other two await decisions)

LANE B — server/CLI/hosted/space core (cli.rs + server.rs + hosted/* + shell.rs + space.rs + render seam)
  markdown-diagrams           (feature, normal, design-first — inline-SVG pattern OR native mermaid; priority = colour-coded status-DAG (the live project-view). CSP/sandbox-sensitive. NEXT docsite unit — awaits Jari's go to start)
  docsite-autolink-convention (feature, low — mostly docs/producer-convention; aggountant keeps a thin preprocessor. Tracked, no rush)
  hosted-submit-return-broken   (bug, high — ANALYZED: by-design/UX gap, submit path is correct; hosted delivery needs a live agent consumer. AWAITING Jari's fix/defer decision (docs+list vs async webhook))
```
<!-- execution-dag:end -->

## Adjacent backlog (not in a lane)

- `hosted-multiworker-credentials` (low, **deferred**) — FUTURE secure credential model
  for many publishers; a constraint on publish-first-surface (`api_key` indirection), not
  a scheduled unit. Revisit before rolling hosted publish out to a team.

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
