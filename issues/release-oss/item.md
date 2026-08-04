---
created: 2026-07-25
updated: 2026-07-25
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

## Notes
- No MCP — the `mcp-integration` epic was closed obsolete (2026-07-24). glasspad
  is a CLI tool.
- The old `finalization-release` epic is closed obsolete; its process-management
  items are re-captured above.

