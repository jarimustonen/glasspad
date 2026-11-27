---
created: 2026-11-27
updated: 2026-11-27
type: bug
status: open
priority: normal
provenance: other
provenance_detail: release gate run
source_ref: taskfleet:01m8ergvvps7d4t0ztjf119wap/flake:nav-chrome-frame-swap
originating_run: 01m8ergvvps7d4t0ztjf119wap
originating_run_kind: spinoff
lane: unlaned
---

# Security probe nav-chrome frame-swap check races the iframe navigation

## Description

Observed during the 0.21.0 release gate: one run of `./test-security.sh` failed with

    FAIL  nav-chrome: clicking a nav entry swapped the framed artifact in place  — iframe src should be /demo/_c/eval

and the two immediately following runs on the same tree passed all checks.

In `tests/security/run.mjs` (around the "Clicking a parent-nav entry swaps the iframe in place" block) the probe waits for `#gp-artifact`'s `src` attribute to name the new page, swallows a timeout, and then asserts on `page.frames()` URLs. The attribute can be updated before the frame has committed its navigation, so the frame-URL assertion races the load. Waiting on the frame itself (for example `page.waitForFrame` / polling `page.frames()` for the target URL) would remove the race.

Impact: a spurious red gate during a release cut; the release gate requires a green `./test-security.sh`, so a flake here costs a rerun and judgment about whether the failure is real.
