---
created: 2026-08-20
updated: 2026-08-21
type: improvement
status: done
priority: high
lane: cli-structure
lane_seq: 10
collision: [crates/glasspad-cli/src/cli.rs]
commits:
- hash: 674fe23f2405999b2696ce04acee7c1ea67d6483
  summary: mark CLI module split in progress
- hash: c2246147bab99f030164313857c2fcf0f4d673c0
  summary: mark CLI module split in progress
- hash: ab7f2fa
  summary: split CLI into coherent command modules
closed: 2026-08-21
---

# Split the 5000-line cli.rs into per-command-group modules

## Description

`crates/glasspad-cli/src/cli.rs` is **5049 lines**. The `cli-canon-s22` core/cli split
(landed 2026-08-20) did not shrink it at all — that work moved crate boundaries, which is a
different axis. This file is the repo's actual maintenance pain point: nearly every unit in
the 2026-08-17..20 rounds touched it, which is precisely why those units had to be sequenced
into one lane instead of running in parallel.

**Do:** split it into per-command-group modules (e.g. publish, serve/host-serve, submissions,
config, skill, doctor, help, version), leaving `cli.rs` as a thin dispatch/wiring layer.

## Why this is priority high

This is not canon compliance — the AI-first CLI canon does not ask for it. It is chosen on its
own merits (Jari, 2026-08-20): the file's size is what forces serialization of otherwise
independent work. Splitting it widens the parallelism available to every future round, so it
pays for itself across subsequent stints rather than delivering user-visible value itself.

Placed at the **head of the DAG** deliberately: doing it first makes the work that follows
cheaper, including `artifact-host-core-extract`.

## Constraints

- **Behaviour must be identical.** This is a move, not a rewrite. Do not rename commands,
  change flags, alter output, or "improve" logic in passing — that converts a reviewable move
  into an unreviewable rewrite.
- The CLI surface is covered by integration tests that invoke the built binary
  (`CARGO_BIN_EXE_glasspad`); they must pass unchanged.
- Do not disturb the `cli-canon-s22` boundary: pure logic belongs in `glasspad-core`, the clap
  surface and I/O stay in `glasspad-cli`. This issue only reorganizes *within* the cli crate.
- Preserve the single published `glasspad` package and the installed `glasspad` binary
  (`cargo publish --dry-run` + `cargo install --path .` must both still work).

## Done criteria

- No single module in the cli crate is disproportionately large; `cli.rs` is thin dispatch.
- `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` green
  (run tests 3x — this area recently had a flakiness fix that must stay stable).
- FULL `./test-security.sh` green (Phase 1 + Wave 2a).
- `CHANGELOG.md` line under `[Unreleased]`, described as an internal change with no
  user-visible behaviour.

## Provenance

Filed 2026-08-20 after `cli-canon-s22` landed and measurement showed the core/cli split left
`cli.rs` untouched at 5049 lines. Jari's call: do this, and put it at the head of the DAG.
