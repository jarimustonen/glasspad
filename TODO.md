# TODO — Glasspad handoff

Orchestrator entry point for `/stint`. This is the round-by-round plan; the
authoritative detail lives in the issue tracker (`issuectl`), not here.

## Where we are

**glasspad 0.3.0 is FULLY RELEASED** (2026-08-09) — crates.io + GitHub Release
`v0.3.0` (mac + 2× Linux binaries, installer, `glasspad.rb` Homebrew formula) +
Homebrew tap, all live. The agent→HTML consolidation. `main` carries 0.3.0
(Cargo.toml) + tag `v0.3.0`. Prior baseline: 0.2.1 (2026-08-05, all three channels).

The GitHub-Release completion (blocked since 2026-08-06 by hauis's mac job failing)
was finished 2026-08-09 by rerunning the failed job on hauis after a **durable
runner-gitconfig fix** — see below.

**Round 2026-08-09/10 (this session) — 4 units landed on main, full green gate
(fmt/clippy/test + `./test-security.sh` 41 + Wave 2a), issue tracker now empty:**
- ✅ `hosted-config-path-macos` — `publish` honors `$XDG_CONFIG_HOME`/`~/.config` on all
  platforms (matches `--help`); old `dirs::config_dir()` path still read as fallback.
- ✅ `hosted-noindex-missing` — `X-Robots-Tag: noindex, nofollow` on hosted read routes
  (host-serve only; loopback `serve` untouched) + regression test.
- ✅ `particularly-offbeat-dust` — optional `idempotency_key` on `POST /api/v1/pages`
  (per-tenant scoped, fsync + atomic mapping; no key → today's behaviour byte-for-byte).
- ✅ `mac-release-self-hosted` — **reverted** the mac release build `macos-14` →
  self-hosted `hauis`, now that hauis is durably fixed and is the intended mac machine.

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

## ▶ Start here (on return)

**Scheduled: `artifact-return-channel` (Lane E, head-of-line).** 0.3.0 fully released; the
2026-08-09/10 round's four units all landed and are green; `main == origin`, clean. The next
build is the return-channel feature — **direction decided (hosted target, loopback rides
along)**, but two sub-choices are settled at/before build from
`issues/artifact-return-channel/models-comparison.md` (consumption transport; one-shot vs
multi-round). Read `design.md` + `models-comparison.md` first; it's a big multi-file unit that
touches the frozen security boundary, so it needs new Wave security cases and review
(`/worktree-code` or a design-first spinoff with `/llm-review`). *Optional polish* below is
lower priority.

_Resolved this round:_ the macOS→`macos-14` routing question — **reverted to self-hosted
`hauis`** (`mac-release-self-hosted`, done). Mac release builds run on hauis again; the
runner is durably fixed (see below), so the next tag push will build mac on hauis.

### Optional polish (no hard gate)

- **Cosmetic confirms:** LICENSE holder "Jari Mustonen"; SECURITY.md / CoC contact
  `jari@itsellesi.fi`. Change via normal edit + patch release if wanted.
- **Close the `release-oss` epic** — effectively complete.
- `version-commit-stamp` follow-up (recorded in `history/assessment-version-commit-stamp.md`):
  read `.cargo_vcs_info.json` for crates.io-tarball provenance. Low.
- Next forward work: downstream homebase + tilictl consolidation, gated on 0.3.0 (tracked there).

## Execution DAG (2026-08-10)

Scheduling PLAN — source of truth for lane + order; issuectl is authoritative for STATUS
(never copied here). Merge each round (drop landed, add active, keep existing order).
`▶` = head-of-line snapshot — RE-COMPUTE from issuectl at pick time.
`after <slug> (needs …)` = logical blocked_by mirror. `collision: <file>` = touches a
second lane's hot file (spawn-time exclusion).

Hot files → lanes: `src/artifact_host/assets/base.css` (design system, Lane A);
`src/cli.rs` + `src/server.rs` + render modules (Lane B). `src/skill.md` is docs-only.
Lane E (artifact return channel) spans `bridge.js` + `src/artifact_host/{headers,mod}.rs`
+ `src/server.rs` + `src/hosted/` ingest + `src/cli.rs` — so it **collides with Lanes A/B**;
do not run it in parallel with work on those files.

<!-- execution-dag:begin -->
```
GLOBAL HEAD-OF-LINE: artifact-return-channel

LANE E — artifact return channel
  ▶ artifact-return-channel   collision: bridge.js/headers.rs/mod.rs/server.rs/hosted/cli.rs
```
<!-- execution-dag:end -->

**Lane E — `artifact-return-channel` (head-of-line, scheduled 2026-08-10).** Hosted
form/input back to the creating agent via the trusted shell as an airlock; the artifact
sandbox stays frozen (`connect-src 'none'`). Direction DECIDED — hosted target, loopback
rides along. Two sub-choices settled at/before build (pro/cons + recommendation in
`issues/artifact-return-channel/models-comparison.md`): consumption transport (rec: A1 poll
+ A3 `await-submission`) and one-shot vs multi-round (rec: B1 one-shot + versioned submission
record). Design: `issues/artifact-return-channel/design.md`. Big multi-file unit spanning
`bridge.js`, `src/artifact_host/{headers,mod}.rs`, `src/server.rs`, `src/hosted/`, `src/cli.rs`
— collides with Lanes A/B, don't parallelise. NEW Wave security cases mandatory; use
`/worktree-code` (reviewed) or a design-first spinoff with `/llm-review`.

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
- Downstream homebase + tilictl consolidation is the next forward work, gated on 0.3.0
  (tracked in those repos).

## Verify / deploy (localhost)

Per `CLAUDE.md`: after editing host code or a base lib, `cargo build`, restart
`glasspad serve`, reload the space. `./test-security.sh` (41 browser checks +
Wave 2a probes) is the regression gate after any host/header/CSP/bridge change.
Use `./test-browser.sh` (check `./test-browser.sh errors` first) for ad-hoc
browser automation.

## Notes for the orchestrator

- Keep `main` clean — commit issue/status changes immediately (`CLAUDE.md`).
- The repo is public now; treat commits/history as public.
- `/oss-*` skills (over `ossctl`) drive release/readiness work; `ossctl audit` scores gaps.
- Track all planning under the issue, not as loose files.
