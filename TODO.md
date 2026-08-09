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
- ✅ `release-mac-github-runner` — mac release build moved off self-hosted `hauis`
  → GitHub-hosted `macos-14` (`dist-workspace.toml`); future releases don't touch `hauis`.

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

### ⚠️ ▶ Follow-up decision for Jari — revert the macOS→`macos-14` routing?

`release-mac-github-runner` (commit `9deee1a`, on `main`) routes future mac builds to
GitHub-hosted `macos-14` in `dist-workspace.toml`. That was done believing hauis was
unreliable — now that hauis is durably fixed and is **the intended mac build machine**,
main's config contradicts intent: the *next* release would build mac on `macos-14`, not
hauis. If you want hauis to own mac builds again, revert that routing back to
`aarch64-apple-darwin = "self-hosted"` (a small `dist-workspace.toml` change + `dist
generate` to regenerate `release.yml`). Orchestrator can spawn a worktree for it on your
go. (v0.3.0 itself already built on hauis — its tag predates 9deee1a.)

### Optional polish (no hard gate)

- **Cosmetic confirms:** LICENSE holder "Jari Mustonen"; SECURITY.md / CoC contact
  `jari@itsellesi.fi`. Change via normal edit + patch release if wanted.
- **Close the `release-oss` epic** — effectively complete.
- `version-commit-stamp` follow-up (recorded in `history/assessment-version-commit-stamp.md`):
  read `.cargo_vcs_info.json` for crates.io-tarball provenance. Low.
- Next forward work: downstream homebase + tilictl consolidation, gated on 0.3.0 (tracked there).

## Execution DAG (2026-08-06)

Scheduling PLAN — source of truth for lane + order; issuectl is authoritative for STATUS
(never copied here). Merge each round (drop landed, add active, keep existing order).
`▶` = head-of-line snapshot — RE-COMPUTE from issuectl at pick time.
`after <slug> (needs …)` = logical blocked_by mirror. `collision: <file>` = touches a
second lane's hot file (spawn-time exclusion).

Hot files → lanes: `src/artifact_host/assets/base.css` (design system, Lane A);
`src/cli.rs` + `src/server.rs` + render modules (Lane B). `src/skill.md` is docs-only.

<!-- execution-dag:begin -->
```
GLOBAL HEAD-OF-LINE: hosted-config-path-macos   ← two maalla.dev bugs, awaiting fix/defer call

0.3.0 fully released 2026-08-09 (crates.io + GitHub Release + Homebrew). Two new
hosted-share bugs filed 2026-08-09 from the glasspad.maalla.dev deploy. They touch
disjoint files → parallel-safe.

LANE B — src/cli.rs + render/publish config
  ▶ hosted-config-path-macos   publish config path: --help says ~/.config, macOS reads ~/Library/Application Support
LANE C — src/hosted (host-serve response headers/routes)
  ▶ hosted-noindex-missing     hosted /p/<slug> pages omit documented X-Robots-Tag: noindex
```
<!-- execution-dag:end -->

## How to cut a release (the recipe, now automated)

`git push origin vX.Y.Z` triggers **both** workflows off the tag:
- `release.yml` (cargo-dist) → builds binaries + pushes the Homebrew formula to the tap.
- `publish-crates.yml` → `cargo publish` to crates.io.

Bump the version in `Cargo.toml`, add a `CHANGELOG.md` entry, commit, then tag+push.
**Caveats learned this stint:**
- **The macOS build now runs on a GitHub-hosted `macos-14` runner** (moved off the
  self-hosted `hauis` on 2026-08-07, `release-mac-github-runner`) — no more shared-
  gitconfig git-400. The v0.2.x/v0.3.0 tags predate the switch and still used `hauis`.
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
- **Two open bugs from the glasspad.maalla.dev deploy (2026-08-09), awaiting Jari's
  fix/defer call:** `hosted-config-path-macos` (publish config path help vs macOS
  reality) and `hosted-noindex-missing` (hosted `/p/<slug>` missing `noindex`).
- **Follow-up decision:** revert the macOS→`macos-14` routing so hauis owns mac builds
  again (see "⚠️ ▶ Follow-up decision" above).
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
