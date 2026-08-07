# TODO — Glasspad handoff

Orchestrator entry point for `/stint`. This is the round-by-round plan; the
authoritative detail lives in the issue tracker (`issuectl`), not here.

## Where we are

**glasspad 0.3.0 is PUBLISHED to crates.io** (2026-08-06) — the agent→HTML
consolidation. crates.io `glasspad 0.3.0` is live and permanent. `main` carries
0.3.0 (Cargo.toml) + tag `v0.3.0`. Prior baseline: 0.2.1 (2026-08-05, all three
channels).

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

## ⚠️ ▶ Start here (on return) — FINISH the 0.3.0 GitHub Release (a decision for Jari)

crates.io 0.3.0 shipped, but the cargo-dist **GitHub Release `v0.3.0` and Homebrew
formula did NOT get created**: `release.yml`'s `aarch64-apple-darwin` job failed
(git HTTP 400 on the self-hosted `hauis` runner — stale auth entry in its shared
`~/.gitconfig`; two runs failed identically). Only the two Linux binaries built.
`release.yml` has **no `workflow_dispatch`** (tag-push only). The durable runner fix
(`release-mac-github-runner`) is now landed on `main`, so the mac build will run on
GitHub-hosted `macos-14` — but the **existing `v0.3.0` tag predates that commit**.

**Pick one to complete 0.3.0's GitHub Release + Homebrew (agent did NOT touch the
published tag):**
- **(a) Fix `hauis`, keep the tag.** On the `hauis` seat machine, clear the bad entry
  in `/Users/jari/.gitconfig` (check `git config --global --list | grep -iE 'extraheader|proxy'`),
  then `gh run rerun 31112313027 --failed`. Rebuilds only the mac job on the OLD
  (self-hosted) config → completes Release + Homebrew. Simplest if the runner is handy.
- **(b) Re-point the tag onto the runner fix.** `git tag -f v0.3.0 <commit-with-macos-14-fix>`
  and force-push the tag → the whole release re-runs on GitHub-hosted `macos-14`, no
  `hauis` dependency. Moves a published tag (crates.io re-publish is a harmless no-op).
  Preferred if `hauis` is unavailable. **Cut once, let it finish** (don't re-tag mid-run).

Either way crates.io users already have 0.3.0.

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
GLOBAL HEAD-OF-LINE: (none — no active code work; backlog empty)

0.3.0 published to crates.io 2026-08-06; all feature + infra issues landed and
dropped (incl. release-mac-github-runner). The only OPEN item is a decision, not
code: finish the 0.3.0 GitHub Release + Homebrew — see "⚠️ ▶ Start here" above
(option a: fix hauis + rerun; option b: re-point v0.3.0 tag). No worktree needed
for either — they are release-ops steps for Jari.

No active non-epic issues. Next code round: file new work, then re-populate lanes.
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

- **0.3.0 agent→HTML consolidation — SHIPPED to crates.io 2026-08-06.** All items
  (`markdown-template-render`, `hosted-share-server`, `skill-routing-guidance`,
  `static-build-output`, `serve-process-mgmt`, `version-commit-stamp`) landed. Only
  the GitHub-Release/Homebrew completion decision remains (see "⚠️ ▶ Start here").
- `release-oss` (epic, high) — **effectively DONE**; close it.
- No open feature backlog. Downstream homebase + tilictl consolidation is the next
  forward work, gated on 0.3.0 (tracked in those repos).

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
