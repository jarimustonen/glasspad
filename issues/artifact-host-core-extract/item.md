---
created: 2026-08-20
updated: 2026-08-21
type: improvement
status: done
priority: normal
lane: cli-structure
lane_seq: 20
collision: [crates/glasspad-cli/src/artifact_host]
blocked_by: ['@cli-module-split']
commits:
- hash: 2c68dcf3e346e329d48ce36f62d348d84ad6338d
  summary: 'fix(core): preserve title semantics after extraction'
- hash: 99a19645a2d68436c96103e440b57cf8981ad28a
  summary: 'refactor(core): extract pure artifact host decisions'
- hash: 48b82a5adb7f081753c5174791602aee3d83cb2d
  summary: 'chore(issue): start artifact host core extraction'
closed: 2026-08-21
---

# Move the pure artifact_host rendering/sanitizing logic into glasspad-core

## Description

`cli-canon-s22` (landed 2026-08-20) established the library-first boundary but moved only
`data`, `security`, and `time` into `glasspad-core` — **1674 lines of core against 26917 lines
on the CLI side.** The intended benefit of §22 ("domain logic unit-testable without the CLI
shell") is therefore only fractionally realised.

Measurement taken 2026-08-20 shows where the remaining value is:

| Module | Lines | I/O touchpoints (`std::fs` / `tokio::fs` / `reqwest` / `axum`) |
|---|---|---|
| `hosted` | 8481 | **280** |
| `artifact_host` | 7342 | **33** |

**Do:** move the pure parts of `artifact_host` — HTML wrapping, sanitization, shell rendering,
template application — into `glasspad-core`, leaving only the genuinely I/O-bound edges in the
CLI crate.

## Why this part specifically

`artifact_host` is nearly pure logic *and* it is the security-critical surface: it is what
decides how untrusted author HTML gets wrapped, sanitized, and sandboxed. Making it testable
without spawning a server is the single highest-value piece of the remaining §22 work — the
adversarial suite currently has to drive a real server for checks that are, at heart,
decisions about strings.

## Explicitly out of scope: `hosted` stays where it is

**Decision (Jari, 2026-08-20): `hosted` is NOT moving to core, now or later.** With 280 I/O
touchpoints it is a durable on-disk store plus an HTTP surface — it *is* the shell that §22
means by "the I/O edge". Relocating it into a no-I/O core would require inverting the whole
design for no benefit.

Recorded here so a future canon audit does not re-raise it: a §22 conformance report that
flags `hosted` as an unmoved domain module should be **rejected on these grounds**, not
actioned.

## Constraints

- **Behaviour must be identical**, and the security contract must not shift by even one
  header, CSP directive, or sandbox token. This is a move.
- `glasspad-core` must remain pure: **no `clap`, no `std::fs`, no `SystemTime::now()`**. Time
  comes from the injected `Clock` trait (`crates/glasspad-core/src/time.rs`). Verify by grep,
  not by inspection.
- Preserve the single published `glasspad` package and the installed `glasspad` binary.
- Prefer `cli-module-split` landing first — it is ahead of this in the lane and makes the
  surrounding code easier to move.

## Done criteria

- The pure rendering/sanitizing logic lives in core with unit tests that need no server.
- Grep proves core still has no clap / no direct filesystem access / no wall-clock reads.
- `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` green
  (3x).
- **FULL `./test-security.sh` green — Phase 1 AND Wave 2a.** Non-negotiable: this moves the
  code that suite exists to guard.
- `CHANGELOG.md` line under `[Unreleased]`, described as internal with no user-visible change.

## Provenance

Filed 2026-08-20 from the post-`s22` measurement above. Jari's call: do the `artifact_host`
extraction, leave `hosted` alone, and do not pursue "§22 to completion" as a goal in itself.

## Resolution

### 2026-08-21T09:46:45Z · @issuectl

Extracted pure artifact-host rendering, sanitization, shell/template, CSP-policy, favicon, and content-version decisions into glasspad-core. Full formatting, clippy, three consecutive cargo test runs, publish dry-run, strict rustdoc, core-purity grep, focused browser smoke, and the complete Phase 1 + Wave 2a security suite are green. Multi-model review was assessed; the one confirmed semantic delta and documentation findings were fixed.
