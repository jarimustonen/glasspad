---
created: 2026-08-23
updated: 2026-08-23
type: task
status: in-progress
priority: normal
provenance: other
provenance_detail: Fleet product rename assigned by orchestratectl
source_ref: orchestratectl:01m0qg3nqtv4ms8e5y5rwzpv6k/task
originating_run: 01m0qg3nqtv4ms8e5y5rwzpv6k
originating_run_kind: spinoff
assignee: orchestratectl:01m0qg3nqtv4ms8e5y5rwzpv6k
---

# Migrate active ossctl references to Shipshape

## Description

Migrate Glasspad's active release/readiness tooling references from ossctl to Shipshape under accepted Shipshape ADR-0005.

## Scope

- Audit every repository match for `ossctl` semantically.
- Rename active commands, skill references, and current product guidance to `shipshape` / `/shipshape-*`.
- Preserve historical records and ADR-0005 compatibility identifiers unchanged.
- Run Glasspad's full green gate and document deliberately retained references.

## Acceptance criteria

- [ ] All active product and CLI references use Shipshape.
- [ ] Historical and compatibility references remain accurate.
- [ ] Full repository green gate passes.
- [ ] Changes are committed and merged.
