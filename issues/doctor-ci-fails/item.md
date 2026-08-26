---
created: 2026-08-25
updated: 2026-08-26
type: bug
reporter: jari
status: fixed
priority: high
commits:
- hash: af4ceba
  summary: fix bundled skill version alignment and add drift guard
closed: 2026-08-26
---

# Doctor CI fails on bundled skill metadata mismatch

## Description

## Description

The `CI` workflow fails in `tests/doctor_cli.rs` because `glasspad doctor` reports `skill.bundle` as failed: the bundled `glasspad` skill metadata does not match the running CLI catalog. The failure affects both the all-green/read-only test and the API-key-redaction test because both expect `doctor` to exit successfully.

Latest failing run: https://github.com/jarimustonen/glasspad/actions/runs/32714438486

## Reproduction

```sh
cargo test --test doctor_cli -- --nocapture
```

A direct hermetic invocation reports:

```json
{"id":"skill.bundle","status":"fail","message":"bundled skill \"glasspad\" metadata does not match the running CLI catalog","fix_suggestion":"Reinstall glasspad from a verified release."}
```

The workflow log shows:

```text
test doctor_all_green_is_read_only ... FAILED
test doctor_never_prints_api_key_material ... FAILED
error: test failed, to rerun pass `--test doctor_cli`
```

## Root cause

The checked-in bundled skill metadata and the CLI's expected catalog metadata have drifted, likely during the 0.17.1 release/version refresh. The diagnostic correctly detects that mismatch, so tests which expect a clean source-tree build to have a valid bundle fail.

## Proposed fix

Regenerate or update the bundled `glasspad` skill metadata from the current CLI catalog, add a test or release check that compares the checked-in bundle to the generated catalog before cutting a release, and verify `cargo test --test doctor_cli` plus the full CI suite.

## Quick Test

- `cargo test --test doctor_cli`
- `cargo test --all-targets`
- Confirm `glasspad doctor --json` reports `skill.bundle` as `ok` in a hermetic home.

## Implementation

Kept the checked-in version metadata rather than adding runtime substitution: `skill print`
and `skill install` deliberately expose the exact bundled `skill.md` bytes, so substitution
would add allocation and a second representation seam to otherwise static content. The
metadata is updated to 0.17.1, and a focused fast test now compares it with
`CARGO_PKG_VERSION` using an explicit version-bump failure message. Release policy also
requires rerunning the full gate after updating both versions.

## Acceptance Criteria

- [x] `glasspad doctor --json` reports `skill.bundle` as healthy in a hermetic home.
- [x] Bundled skill metadata has a focused package-version drift guard.
- [x] Formatting, clippy, doctor integration tests, and the full test suite pass.
