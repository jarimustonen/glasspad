---
created: 2026-07-25
updated: 2026-08-04
type: epic
status: open
priority: high
---

# Release glasspad 0.2.0 as a proper open-source project

## Description

Ship glasspad **0.2.0** as a proper open-source project. The v0.2 rewrite
(HTML-artifact host) is complete and green on `main`; this epic turns it into
a released, installable, public OSS project.

**PO decision (2026-07-25):** publish across **all three** channels — not one.

> ⚠️ **Execution gate.** This epic is intended to be executed by a new
> **`/oss-*` skill family** (`/oss-release`, `/oss-…`) that does **not exist
> yet**. The next `/stint` must FIRST check whether those skills are installed.
> If they are not, the stint STOPS and warns the user — no release work proceeds
> until the `/oss-*` skills are in place. See `TODO.md` "Start here".

## Publish channels (all three)

- **(a)** Git **tag + `cargo install --git`** — simplest path, fits a
  loopback-only tool.
- **(b)** **crates.io** — `cargo publish` (needs complete package metadata:
  description, license, repository, keywords, categories, readme).
- **(c)** **Homebrew tap + GitHub release binaries** — prebuilt artifacts +
  a shell installer, in the style of `issuectl` / `orchestratectl` (likely
  `cargo-dist` or equivalent).

## Scope

### Must (release blockers)
- **Reconcile version → `0.2.0`.** `Cargo.toml` says `0.1.0` while
  `src/skill.md` `cli_version` says `0.2.0`; the CLI's `--json` envelopes now
  surface `cli_version` (e.g. `skill --install-claude --json`), so the mismatch
  is in the machine-readable contract. Single source of truth.
- **LICENSE** — pick and add an OSS license (MIT / Apache-2.0 / dual — PO call).
- **crates.io package metadata** in `Cargo.toml` (description, license,
  repository, homepage, keywords, categories, readme, authors).
- **CHANGELOG.md** — 0.2.0 entry summarizing the rewrite.
- **Green gate before tagging:** `cargo build`, `cargo clippy --all-targets`,
  `cargo test`, `./test-security.sh` all pass.
- **Install verification** — `cargo install --path .` (and `--git`) works from a
  clean checkout / another repo; the installed binary runs `serve`/`create`/
  `open`/`skill`.

### OSS hygiene (proper-project polish)
- **CONTRIBUTING.md**, issue/PR templates, `README` badges (crates.io, CI,
  license), a repo description + topics.
- **CI** — GitHub Actions running build/clippy/test (+ ideally the security
  suite) on push/PR.
- **Release automation** — tag → build binaries → GitHub release + Homebrew tap
  formula bump + crates.io publish.

### Round-it-out (decide: include in 0.2.0 or defer to 0.2.1)
- Process-management gaps from the old `finalization-release` epic that are
  legit for a released CLI: **`glasspad stop`**, **`GLASSPAD_PORT`** env var,
  **PID file** (`~/.glasspad/server.pid`). Recommend deciding at stint start;
  default lean: defer to 0.2.1 so 0.2.0 ships the current green surface.

## Progress (2026-08-04) — OSS readiness complete via the `/oss-*` family

The `/oss-*` skills are installed (thin wrappers over `ossctl` 0.1.0). Ran the family:
`ossctl audit` now reports **`core_complete: complete`, 0 gaps**. Done:

- ✅ **`OSS-RELEASE.md`** contract — approved, `maturity: mvp`, `license: MIT` (PO call).
- ✅ **Version → `0.2.0`** — `Cargo.toml` bumped (was `0.1.0`); matches `src/skill.md`.
- ✅ **LICENSE** — MIT (`/oss-readme`).
- ✅ **crates.io metadata** — description, license, repository, homepage, readme,
  keywords, categories, `exclude`. `cargo publish --dry-run` passes (87 files).
- ✅ **CHANGELOG.md** — `[0.2.0]` entry (Keep a Changelog, curated).
- ✅ **Green gate** — `cargo fmt --check` (repo-wide fmt applied), `clippy`, `test` all
  green. `./test-security.sh` still to be run as the pre-tag regression gate.
- ✅ **README badges** (CI + license), **CONTRIBUTING.md**, **CODE_OF_CONDUCT.md**
  (Contributor Covenant 2.1), **PR template**, **SECURITY.md** (full mvp policy).
- ✅ **CI** — `.github/workflows/ci.yml` (fmt/clippy/test) + `.github/dependabot.yml`.
- ✅ **Default branch** renamed `master → main` (local; remote flip pending push).

### 🚀 0.2.0 SHIPPED (2026-08-05)
- ✅ **Pushed** `main`, flipped GitHub default → `main`, deleted `origin/master`.
- ✅ **`./test-security.sh`** green (41 + Wave 2a) as the pre-tag gate.
- ✅ **Tagged `v0.2.0`** + **GitHub Release** created (CHANGELOG notes).
- ✅ **Published to crates.io** — `glasspad 0.2.0` is **live** (`cargo install glasspad`).
  Publish ran **in CI** via `.github/workflows/publish-crates.yml` on release-published,
  using the `CARGO_REGISTRY_TOKEN` repo secret (provisioned from
  `infra/secrets/crates-io.yaml`) — same mechanism as ossctl/issuectl. (A local
  `cargo publish` 403'd on a stale `~/.cargo/credentials.toml`; CI is the correct path.)

### 🚀 0.2.1 SHIPPED — all three channels live (2026-08-05)
- ✅ **crates.io** `glasspad 0.2.1`, **GitHub Release v0.2.1** (macOS arm64 + Linux
  arm64/x86_64 binaries, checksums, build-provenance attestations, installer script), and
  **Homebrew** `brew install jarimustonen/glasspad/glasspad` (formula in
  `jarimustonen/homebrew-glasspad`).
- ✅ **cargo-dist** wired (`dist-workspace.toml` → `release.yml`); macOS on self-hosted
  `hauis`; `HOMEBREW_TAP_TOKEN` set (via `gh auth token`).
- Bumps hit along the way (all fixed): missing `[profile.dist]`; unused `reqwest` pulling
  `openssl-sys` (broke arm64-linux cross-compile) → removed; hauis git-400 from overlapping
  mac jobs (rapid re-tag) → cleaned + run-once; empty tap repo (no `main`) → initialized;
  repo made **public** (needed for attestations); crates trigger moved to tag-push (the
  GITHUB_TOKEN release event never fired `publish-crates.yml`).
- **Release recipe now:** `git push origin vX.Y.Z` → release.yml (binaries + Homebrew) +
  publish-crates.yml (crates.io), both on the tag push. Cut a tag once; don't re-tag while a
  hauis mac job is in flight.

### Still open (polish — not release-blockers)
- **Confirm placeholders:** LICENSE holder ("Jari Mustonen"), SECURITY/CoC contact
  (`jari@itsellesi.fi`) — cosmetic.
- **Round-it-out items** (`glasspad stop`, `GLASSPAD_PORT`, PID file) — deferred to 0.2.2.

## Notes
- No MCP — the `mcp-integration` epic was closed obsolete (2026-07-24). glasspad
  is a CLI tool.
- The old `finalization-release` epic is closed obsolete; its process-management
  items are re-captured above.

