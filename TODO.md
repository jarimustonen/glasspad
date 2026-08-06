# TODO — Glasspad handoff

Orchestrator entry point for `/stint`. This is the round-by-round plan; the
authoritative detail lives in the issue tracker (`issuectl`), not here.

## Where we are

**glasspad 0.2.1 is RELEASED as an open-source project** (2026-08-05), across
all three channels. The v0.2 HTML-artifact-host rewrite was already complete;
this stint turned it into a public, installable OSS project via the `/oss-*`
skill family (thin wrappers over the `ossctl` binary).

- **Published:** crates.io `glasspad 0.2.1`; GitHub Release `v0.2.1` with
  prebuilt binaries (macOS arm64 + Linux arm64/x86_64, checksums, build-provenance
  attestations, installer script); Homebrew —
  `brew install jarimustonen/glasspad/glasspad` (tap `jarimustonen/homebrew-glasspad`).
- **The repo is now PUBLIC** (`github.com/jarimustonen/glasspad`), default branch
  **`main`** (renamed from `master`).
- **OSS scaffolding in place:** `OSS-RELEASE.md` (approved, mvp, MIT), LICENSE (MIT),
  README (badges + install), CI (`ci.yml` fmt/clippy/test + dependabot), CHANGELOG,
  CONTRIBUTING, CODE_OF_CONDUCT, SECURITY.md, and cargo-dist (`dist-workspace.toml`
  → `release.yml`). `ossctl audit` = `core_complete`, 0 gaps.
- **Green baseline on `main`:** `./test-security.sh` = 41 browser checks + Wave 2a
  probes; `cargo fmt --check` / `build` / `clippy --all-targets` / `test` all clean.

## ▶ Start here (next session) — the 0.3.0 agent→HTML consolidation

**This is the substantive forward work now.** Glasspad becomes the single
canonical "agent → browser HTML" surface, absorbing the roles today split across
publish-html (persistent shareable pages), tw view (seat preview), and the
tilictl `build_docs.py` docsite pattern. Full design + rationale: homebase
`issues/glasspad-html-consolidation/design.md` (Option D — hosted share server).

**Target: 0.3.0.** Filed issues (critical path in order):

1. **`prose-theme`** (feature, high) — prose/reading theme in the `--gp-*` system.
2. **`markdown-template-render`** (feature, high) — md + reusable-template render
   (server-side; payload = markdown + template reference). The headline feature.
3. **`hosted-share-server`** (feature, high) — hosted run mode: API-key ingest,
   capability-slug public URLs, retention, multi-tenant; loopback `serve` unchanged.
4. **`skill-routing-guidance`** (task, normal) — skill: when serve vs publish vs preview.
5. **`static-build-output`** (feature, low, optional) — `glasspad build` static render.

Order: `prose-theme` → `markdown-template-render` → `hosted-share-server` →
`skill-routing-guidance`; `static-build-output` is optional/parallel. Downstream
homebase + tilictl work is gated on **0.3.0 landing** (tracked in those repos).

### Optional polish (no hard gate, unchanged)

- **Round-it-out CLI features** (deferred to a **0.2.2** patch, independent of the
  above): `glasspad stop`, `GLASSPAD_PORT` env var, PID file (`~/.glasspad/server.pid`).
- **Cosmetic confirms:** LICENSE copyright holder "Jari Mustonen"; SECURITY.md /
  CoC contact `jari@itsellesi.fi`. Change via a normal edit + patch release if wanted.
- **Close the `release-oss` epic** — effectively complete (all three channels shipped).

## How to cut a release (the recipe, now automated)

`git push origin vX.Y.Z` triggers **both** workflows off the tag:
- `release.yml` (cargo-dist) → builds binaries + pushes the Homebrew formula to the tap.
- `publish-crates.yml` → `cargo publish` to crates.io.

Bump the version in `Cargo.toml`, add a `CHANGELOG.md` entry, commit, then tag+push.
**Caveats learned this stint:**
- **Cut a tag ONCE and let it finish.** The macOS build runs on the self-hosted `hauis`
  runner; overlapping mac jobs (from rapid re-tagging) collide on the shared global
  gitconfig → git 400. Don't re-tag while a mac job is in flight.
- Secrets already set on the repo: `CARGO_REGISTRY_TOKEN`, `HOMEBREW_TAP_TOKEN`.
- The agent has **standing release autonomy** (see `AGENTS.md` → Operating Policy):
  may cut + publish releases autonomously, gated on green checks, including deciding
  to release. Pushing/publishing is pre-authorized.

## Backlog

- **0.3.0 agent→HTML consolidation** — `prose-theme`, `markdown-template-render`,
  `hosted-share-server`, `skill-routing-guidance`, `static-build-output` (see ▶ Start
  here). The primary forward work.
- `release-oss` (epic, high) — **effectively DONE** (0.2.1 shipped all three channels);
  close it or retain only for the 0.2.2 round-it-out items.
- 0.2.2 round-it-out: `glasspad stop`, `GLASSPAD_PORT`, PID file (deferred, not started).

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
