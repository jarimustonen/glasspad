---
created: 2026-08-23
updated: 2026-08-23
type: task
status: done
priority: normal
provenance: other
provenance_detail: Fleet product rename assigned by orchestratectl
source_ref: orchestratectl:01m0qg3nqtv4ms8e5y5rwzpv6k/task
originating_run: 01m0qg3nqtv4ms8e5y5rwzpv6k
originating_run_kind: spinoff
assignee: orchestratectl:01m0qg3nqtv4ms8e5y5rwzpv6k
commits:
- hash: 2d9a0f9
  summary: 'chore(release): migrate active tooling references to Shipshape'
closed: 2026-08-23
---

# Migrate active ossctl references to Shipshape

## Description

Migrate Glasspad's active release/readiness tooling references from ossctl to Shipshape under accepted Shipshape ADR-0005.

## Scope

- Audit every repository match for `ossctl` semantically.
- Rename active commands, skill references, and current product guidance to `shipshape` / `/shipshape-*`.
- Preserve historical records and ADR-0005 compatibility identifiers unchanged.
- Run Glasspad's full green gate and document deliberately retained references.

## Acceptance Criteria

- [x] All active product and CLI references use Shipshape.
- [x] Historical and compatibility references remain accurate.
- [x] Full repository green gate passes.
- [x] Changes are committed and merged.

## Decisions

### 2026-08-23T14:45:10Z · @pi-worker

Migration complete and green. Retained ossctl matches are limited to historical issue records, the migration record itself, the unchanged jarimustonen/ossctl repository coordinate required by ADR-0005, and the historical /oss-init generation provenance. OSS-RELEASE.md and oss-changelog:* remain permanent compatibility identifiers. Full gate passed: fmt, clippy -D warnings, cargo test, cargo publish --dry-run, and test-security.sh (48 checks plus Wave 2a).

### 2026-08-23T14:46:29Z · @pi-worker

Because fleet convergence is conductor-owned and no global shipshape binary is installed yet, ran the canonical Shipshape source in a disposable /tmp Cargo target. `shipshape audit --json` reported `core_complete: complete` with zero gaps; the temporary build directory was removed.

