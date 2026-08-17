# TODO — Glasspad handoff

Orchestrator entry point for `/stint`. This is the round-by-round plan; the
authoritative detail lives in the issue tracker (`issuectl`), not here.

## Where we are

**CURRENT: 0.14.0 is released + live; three units have landed on `main` since and are
UNRELEASED.** `Cargo.toml` still says `0.14.0`; `CHANGELOG.md [Unreleased]` is written and
describes what a `0.15.0` would ship. Jari's steer at the 2026-08-16/17 handoff: **do the
next stint first, then release** — so 0.15.0 is deliberately not cut yet, and cutting it is
the natural first or last act of the next round. Full green gate was verified on `main`
after the three units integrated (see the round entry below). The 0.3.0→0.14.0 release
history is preserved below for context.

## Round 2026-08-16/17 — issue-queue triage + 3 parallel lanes landed (UNRELEASED)

A triage-led round. The queue was audited before any code was written, then three lanes ran
in parallel. **Nothing released** — see "CURRENT" above.

**Triage: the queue was two-thirds noise.** Of 15 open issues, six came from the single
`hosted-store-generation-pointer` review panel. **Four were closed `wontfix` without code**
(`b55bca0`), each disqualified by justification the panel had itself written into the issue
body: `hosted-store-input-revalidation` ("the HTTP layer validates and the store is
server-private"), `hosted-loadbudget-asset-caps` (over-cap assets "re-rejected downstream by
build_space_bundle"), `hosted-genptr-autoheal` ("won't happen on ext4 default", loses no
data), `hosted-multiworker-credentials` (designs for a team that does not exist — its design
constraint stays recorded in `issues/publish-first-surface/design.md`; refile if a second
publisher appears). **Jari's rule, standing:** an issue whose own text says "another layer
already validates this" or "does not happen on default settings" is not work. Filed upstream
as homebase `triage-plausibility-filter` so `/triage-unlaned-issues` stops surfacing this
class as lane-able.

**Three units landed green (all self-merged, all verified by content on `main`):**
- ✅ `cli-canon-config` (`049a5ba`+`86a1c48`) — `glasspad config path` / `config show`
  (`--json`), reporting each effective value plus its provenance (flag / env / config file /
  default). **`api_key` is reported only as `<set>`/`<unset>`** — never the secret, enforced
  by `tests/config_cli.rs:60`. This gap was validated in-session: finding the effective
  config cost four guessed paths.
- ✅ `hosted-snapshot-arc-sharing` (`a74f9ec`) — `Snapshot.spaces` is now
  `BTreeMap<String, Arc<Space>>`, so publish/update/round-push no longer deep-copy every
  body under the mutation lock; `MAX_PAGES` now enforced on scan/load too. **Scope was
  deliberately trimmed:** the issue also bundled narrowing the global mutation lock's
  critical section — that was cut from the brief as crash-consistency-sensitive work
  deserving its own unit and review. **It was never filed** (the worker returned an empty
  `spinoff_proposals`), so it exists only in this note — Jari's open call, see Start here.
- ✅ `space-custom-template` (`96dd8a3`+`0073173`) — a space can declare a producer-supplied
  template applied to every markdown page; grouped sidebar, landing index, and TOC rail all
  still work, and a space declaring no template renders byte-identically to before. Note this
  landed **without** the `base-templates-design` process the prior handoff recommended.

**Integrated green gate on `main`** (run under `rustup run stable`): `cargo fmt --all
--check` + `cargo clippy --all-targets -- -D warnings` + `cargo test` (504 tests) +
`./test-security.sh` (41 + Wave 2a, all PASS).

**Docs/infra fixed this round:** the operating policy's stale "Deploy = localhost. There is
no remote deploy." claim was removed (`3682d6a`) — `host-serve` is a public-bind mode meant
for real deployment, and the false claim actively skewed threat-model reasoning during
triage. The stale local toolchain (`rustup default` pinned to 1.85.0 while dependencies
require ≥1.86, so `cargo build` failed out of the box) was fixed outside the repo; `rustup
default` is now `stable` (1.97.1) and a plain `cargo build` succeeds.

**Process gap found:** none of the three units wrote a `CHANGELOG.md` entry, because the
briefs did not ask for one — `[Unreleased]` was still empty after three landings. Written by
hand at handoff (`cfb5fcd`). **Put a changelog line in every worker brief.**

## Round 2026-08-15 — 0.14.0 shipped (generation-pointer hosted store)

One Lane-B hosted-core spinoff landed on `main`, then shipped as **0.14.0** (`42ffc20`, tag
`v0.14.0`). Integrated/release gate: `cargo fmt --all --check` + `cargo clippy --all-targets
-- -D warnings` + `cargo test` (after the documented build-script clean for the local
`version_cli` false-fail) + `./test-security.sh` (48 checks + Wave 2a) + localhost loopback
render smoke + `ossctl audit` (core complete, 0 gaps) + `cargo publish --dry-run`; CI, crates
publish, cargo-dist Release, and Homebrew formula update all green. Verified live on crates.io,
GitHub Release, and Homebrew.
- ✅ `hosted-store-generation-pointer` (`fa957a2` + `1e04534` + `d41db01`, status **done**)
  redesigned hosted spaces and live overlays around immutable generation directories plus an
  atomically swapped `current` pointer. A crash before the pointer flip keeps the prior served
  generation/round live, completed flips preserve the committed-vs-durable honesty contract,
  legacy flat spaces and two-file live overlays read transparently, and review hardening made
  recovery conservative, symlink-safe, and fail-closed on page/space collisions.
- 🌱 Review follow-ups filed and folded into Lane B: `hosted-idem-sweep-robustness`,
  `hosted-gc-swap-on-partial-fsync`, `hosted-snapshot-arc-sharing`,
  `hosted-store-input-revalidation`, `hosted-loadbudget-asset-caps`, and
  `hosted-genptr-autoheal`.

## Round 2026-08-15 — 0.13.0 shipped (hosted store durability honesty)

One Lane-B hosted-core spinoff landed on `main`, then shipped as **0.13.0** (`7daf1e2`, tag
`v0.13.0`). Integrated/release gate: `cargo fmt --all --check` + `cargo clippy --all-targets
-- -D warnings` + `cargo test` (after the documented build-script clean for the local
`version_cli` false-fail) + `./test-security.sh` (48 checks + Wave 2a) + localhost loopback
render smoke + `ossctl audit` (core complete, 0 gaps) + `cargo publish --dry-run`; CI, crates
publish, cargo-dist Release, and Homebrew formula update all green.
- ✅ `materialize-space-durability` (`84fc491` + `c522e88`, status **done**) fixed the
  fsync-after-swap honesty gap in `materialize_space`: the atomic rename is now the commit
  point, callers distinguish `Durable` vs `Unconfirmed`, and served memory is swapped when
  disk already contains the new tree. Deterministic fault-injection tests pin create/replace
  post-commit fsync failures and keep stable-key mappings honest.
- 🌱 Review follow-up filed and folded into the DAG: `hosted-store-generation-pointer`
  (normal, Lane B) for immutable generation directories + atomically-swapped current pointers
  across spaces, stable-key mappings, and live overlays.

## Round 2026-08-15 — 0.12.0 shipped (stable URL updates + hosted sidebar triage)

Two sequenced Lane-B/render-risk spinoffs landed on `main`, then shipped as **0.12.0**
(`b591037`, tag `v0.12.0`). Integrated/release gate: `cargo fmt --all --check` +
`cargo clippy --all-targets -- -D warnings` + `cargo test` + `./test-security.sh` (48 checks
+ Wave 2a) + `ossctl audit` (core complete, 0 gaps) + `cargo publish --dry-run`; CI,
crates publish, and cargo-dist Release workflows all green.
- ✅ `publish-update-in-place` (`79a6a2b` + `0adf9fd`, status **done**) — `glasspad publish
  --update <slug>` now updates an existing hosted space by capability slug while preserving
  the same `/p/<slug>` URL. Server side is owner-authenticated `PUT /api/v1/spaces/{slug}`:
  missing/foreign slug fails closed, idempotency-key replay semantics stay unchanged, retention
  refreshes on update, and security probes cover owner/cross-tenant/unknown/unauthenticated
  cases. Multi-model review hardening applied before merge.
- ✅ `hosted-nav-loses-sidebar` (`46e9e37`, status **cannot-reproduce**) — current `main`
  already renders grouped sidebar chrome on every hosted page URL and across store reopen;
  regression coverage added. The observed maalla.dev symptom is an ops/stored-metadata issue:
  upgrade hosted glasspad to a current build and re-publish affected spaces so `nav_groups`
  is persisted.
- 🌱 Review follow-up filed and folded into the DAG: `materialize-space-durability` (normal,
  Lane B) for `materialize_space` fsync-divergence/generation-pointer durability and optional
  PUT optimistic concurrency.

## Round 2026-08-14/15 — 0.11.0 shipped (hosted return-channel fix + inline-SVG markdown diagrams)

Two parallel spinoffs (Lane B ∥ Lane render, both headless), both landed green + reviewed,
released as **0.11.0** (`9b34edd`, tag `v0.11.0`). Integrated green gate on main: fmt +
clippy -D + `cargo test` (incl. new hosted-submit tests + commit-stamp, no false-fail after
`rm -rf target/debug/build/glasspad-*`) + `./test-security.sh` (48 checks + Wave 2a, whole
suite green) + `ossctl audit` (core complete, 0 gaps) + `cargo publish --dry-run`.
- ✅ `hosted-submit-return-broken` (`99f75d4`+`66c4115`+`47127cc`, status **fixed**) — the
  worker found it was a **genuine defect**, not just the by-design/UX gap the earlier
  read-only analysis suspected: the hosted return channel now works end-to-end for
  CLI-published (space) pages. New **`glasspad submissions <slug>`** drain command (per-tenant
  scoped, `--json`, paginated; cross-tenant → opaque 404) so a returning/departed agent
  fetches its backlog; `publish` now prints the exact `await-submission` invocation (with the
  configured `--public-host`) + retention note. Multi-page version binding + fail-closed owner
  checks hardened after 4-model `/llm-review` + assessment. **Shipped on the PRAGMATIC scope**
  — the true async/webhook push-to-a-departed-agent is a separate future design issue, still
  **unfiled** (Jari's call whether to file).
- ✅ `markdown-diagrams` (`f077046`+`9aafb35`+`7869355`, status **done**) — inline-SVG diagram
  pattern for markdown spaces via **approach (a)**: the producing agent owns SVG generation and
  embeds it inline; glasspad supplies only theme-aware CSS (`--gp-status-*` done/next/blocked/
  future across all three theme blocks + `.gp-diagram/.gp-node/.gp-edge/.gp-status-*/.gp-legend/
  .gp-chip`). Priority colour-coded status DAG renders end-to-end. Chosen over native mermaid
  (b) because it directly serves the live project-view case, adds **no new JS/eval surface**,
  and requires **zero change to the null-origin sandbox or artifact CSP** (regression-asserted
  through the real content route). 4-model `/llm-review` + assessment applied before merge.
- 🌱 The diagrams worker filed **two new issues** from aggountant project-view: `hosted-nav-loses-sidebar`
  (bug — hosted sub-page loses the grouped sidebar) and `space-custom-template` (feature —
  whole-space branded template). Both folded/handled at this handoff (see DAG + Start here).

## Round 2026-08-14 — 0.8.0, 0.9.0, 0.10.0 shipped (all verified live)

Six consecutive clean tag-releases (mac on hauis). Each: full green gate (fmt + clippy -D
+ cargo test + `./test-security.sh` 48 + Wave 2a) + `ossctl audit` (core complete) +
`cargo publish --dry-run`, then tag-push → both CI workflows.
- **0.8.0** (`366a0cf`, `v0.8.0`) — `space-docsite-nav`: grouped/one-level-nested space nav
  via manifest `groups:`, generated grouped landing index (replaces redirect stub),
  manifest-level companion mapping. **+ fixed a pre-existing CRITICAL iframe sandbox-escape**
  (duplicate `title` attr could smuggle `allow-same-origin`) with a regression test.
- **0.9.0** (`7d39563`, `v0.9.0`) — `loopback-lan-serve`: opt-in `glasspad loopback serve
  --bind <private-IPv4>` makes a served space LAN-reachable (solves "I'm on another LAN
  machine"). DNS-rebinding guard KEPT as an allowlist (foreign Host → 421); wildcard/public
  refused; sandbox/CSP/airlock unchanged. 4-model review + 13 LAN probes. New
  `src/artifact_host/guards.rs` LanExposure.
- **0.10.0** (`e5df540`, `v0.10.0`) — `prose-page-toc`: per-page "on this page" H2/H3 TOC
  rail for prose spaces (native collapsible `<details>`, server-generated slug anchors,
  inside the artifact fragment — no shell/postMessage surface, CSP unchanged). Last
  structural docsite feature for the aggountant `project-view` port.
  - **Process note:** the TOC first landed with a RED gate — the new heading `id`s broke a
    stale Gap-2 security probe (`set -e` aborted Wave 2a). The release was HALTED, the probe
    was made attribute-tolerant (`test-security.sh:465`; no security assertion weakened), the
    full suite re-verified green, THEN 0.10.0 shipped. Worker "48 green" claims must be
    re-verified against the FULL `./test-security.sh` (Phase 1 + Wave 2a), not Phase 1 alone.

### Two feedback items from Jari this session
1. **`hosted-submit-return-broken`** (bug, high) — hosted form submissions don't reach the
   creating agent. **Analyzed (read-only): by-design/UX gap, NOT a code defect** — the submit
   path is correct and stores durably, but hosted delivery needs a live agent consuming
   `await-submission`; a published-and-forgotten page has no consumer. Full triage in the
   issue. Config caveat: confirm live `--public-host` == `https://glasspad.maalla.dev` exactly.
2. **3 project-view FRs** filed by the parallel project-view agent (`380b9df`, authored Jari),
   each respecting the space-docsite-nav boundary: `prose-page-toc` (DONE, shipped 0.10.0),
   `markdown-diagrams`, `docsite-autolink-convention`.

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

Clean tree, nothing in flight. **`main` was pushed to origin at this handoff.** Three units
have landed since 0.14.0 and are **unreleased** — `Cargo.toml` is still `0.14.0` while
`CHANGELOG.md [Unreleased]` describes the 0.15.0 content. **Jari's steer: run the next stint
first, then release.** Cutting 0.15.0 needs only a version bump + `--finalize` of the
changelog + tag push; the green gate was already verified on the current `main`.

**Run the gate under `rustup run stable`** if a plain `cargo` ever fails on MSRV — the
dependency chain (`idna_adapter` → `icu_*`) needs ≥1.86. The machine default was corrected to
`stable` this round, so this should no longer bite.

**Four lanes, four ready heads** (`issuectl dag` is authoritative — this is orientation only):

**1. `repo-hygiene` → `audit-no-user-specifics`** (task, **high** — the only high-priority
item open). The repo is public; audit it for user-specific facts that must not ship, and move
any found into user config. The rule: *overridability does not launder a user-specific
default* — an unset default is still whatever ships in the package; the correct built-in
default is neutral/absent with an actionable error naming the config key. **Concrete lead
already identified:** `dist-workspace.toml`'s `[dist.github-custom-runners]` routes the macOS
release build to the personal self-hosted `hauis` runner, and `AGENTS.md` itself calls that a
"personal / non-standard infra override". That is exactly the pattern the issue forbids.

**2. `cli-canon` → `cli-canon-version-payload`**, then `skill-subcommand`, `doctor`,
`help-json`, `s22`. **Jari's decision 2026-08-16: all six cli-canon items get done**, over an
orchestrator recommendation to close three as ceremony. That recommendation is recorded here
only so the next agent does not re-litigate it: `version-payload` would add
`supported_schemas`/`skills[]` that are a constant (one schema version, one shipped skill in
`src/skill.md`); `skill list` would list that one skill (the `skill` verb with
`--install-claude`/`--user`/`--agent` already exists, so this is smaller than the issue
implies); `s22` is a core/cli crate split of a ~4.5k-line `src/cli.rs` that the canon itself
marks "should" and never a release gate. **The decision is made — build them.**

**3. `hosted-hardening` → `hosted-idem-sweep-robustness`**, then
`hosted-gc-swap-on-partial-fsync`. Both carry `collision:src/hosted/store.rs`, so they
sequence. **Trim `hosted-idem-sweep-robustness` before spawning** (recommended, not yet
done): keep only the real part — `sweep_mappings` deletes a mapping on *any* `read_capped`
error including transient EMFILE/EACCES, weakening exactly-once precisely under load; fix is
to delete only on `NotFound` or an explicit parse/validation failure. **Drop** the symlink
and empty-tenant-reap parts: same speculative class as the four closed this round.
`hosted-gc-swap-on-partial-fsync` is a cheap ordering fix (swap the rebuilt snapshot before
surfacing the post-removal fsync error) but its trigger is a failing disk — low priority,
take it in passing, not as a head.

**4. `space-polish` → `docsite-autolink-convention`** (feature, low). Mostly a DOCS/producer
convention: document the "preprocess markdown before publish" seam, and allow a small set of
author link classes (e.g. `<a class="xref">`) to survive into rendered prose for theming.
glasspad does NOT own glossary/xref logic; aggountant keeps a thin preprocessor. Now that
`space-custom-template` has landed, this is largely a paragraph of docs plus confirming which
classes survive.

**Open decisions carried for Jari (neither blocking):**
1. **File the hosted-snapshot mutation-lock narrowing, or drop it?** Staging/fsync outside the
   lock, holding it only for the pointer flip + snapshot swap. Cut from the arc-sharing brief
   on purpose and **never filed** — it exists nowhere but this file. Orchestrator's view: file
   it. It is a genuine throughput constraint (not speculative), but it touches crash-
   consistency code and deserves its own review.
2. **File the hosted-submit async/webhook push** (push-to-a-departed-agent) as a future design
   issue, or drop it? Carried from earlier stints, still unfiled.

_Older candidates still valid:_ tilictl docsite migration (tracked in tilictl); optional
polish (below).

### Standing lessons for worker briefs

- **Every brief gets a `CHANGELOG.md` line in its done criteria.** Three units landed this
  round without one.
- **Every brief that includes `/llm-review` gets the plausibility filter**: reject findings
  whose own justification is "another layer already validates this", "does not happen on
  default settings", or that require an attacker who already has write access to the server's
  own storage. Without it, the next review round regenerates the four issues closed this round.
- **Prefer frequency over severity** when ranking: "happens on every publish" beat "could
  corrupt data under a rare crash" in every call this round.

### Optional polish (no hard gate)

- **Cosmetic confirms:** LICENSE holder "Jari Mustonen"; SECURITY.md / CoC contact
  `jari@itsellesi.fi`. Change via normal edit + patch release if wanted.
- **Close the `release-oss` epic** — effectively complete.
- `version-commit-stamp` follow-up (recorded in `history/assessment-version-commit-stamp.md`):
  read `.cargo_vcs_info.json` for crates.io-tarball provenance. Low.
- Next forward work: downstream homebase + tilictl consolidation, gated on 0.3.0 (tracked there).

## Scheduling

Canonical scheduling lives in `issuectl` frontmatter (`lane:`, `lane_seq:`, `blocked_by:`, `collision:`). Do not maintain a markdown DAG or adjacent backlog in this file.

Use these views instead:

```bash
issuectl dag
issuectl dag --json
issuectl ls --status open
issuectl ls --status in-progress
```

`TODO.md` is only the session handoff and project notes; issue bodies and `issuectl dag` are the source of truth.

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

## Piialiisan bugiraportit

- [x] 🐛 publish: update a published artifact in place (stable slug) instead of minting a new URL — jari via Telegram → **admitted + renamed** to [`publish-update-in-place`](issues/publish-update-in-place/item.md), folded into Lane B (2026-08-15).
