---
created: 2026-08-17
updated: 2026-08-18
type: bug
status: in-progress
priority: normal
---

# Flaky test: free_port() TOCTOU race makes submissions_cli fail under load

## Description

`tests/submissions_cli.rs` picks a port with `free_port()`, which binds `127.0.0.1:0`,
reads the assigned port, and **immediately releases it** before `host-serve` rebinds. The
helper's own comment concedes the gap:

```rust
/// Grab a currently-free loopback port by binding to :0 and immediately releasing
/// it. A short race window before `host-serve` rebinds is acceptable for tests.
```

Under CPU contention that window widens enough for another process (including a sibling
test in the same file — this suite spawns several servers) to take the port, and the test
fails.

## Observed

Seen once on 2026-08-17 during the `cli-canon-help-json` green gate:
`publish_prints_await_and_drain_invocations_with_configured_host` FAILED in a full
`cargo test` run started immediately after `rm -rf target/debug/build/glasspad-*` (so the
whole crate was rebuilding, maximum load).

Not reproducible on demand: green 3/3 in isolation (`--test submissions_cli`) and 3/3 on
full-suite runs afterwards. Nothing about the failure implicated the help-json change —
it is pre-existing test infrastructure.

## Why it matters

`cargo test` is part of the **release gate**. A test that fails ~1-in-N under load either
blocks a release spuriously or, worse, trains the operator to re-run until green — which
is exactly how a real regression gets waved through. The cost of the fix is low; the cost
of an untrusted gate is not.

## Suggested fix

Hold the listener instead of releasing it: bind `127.0.0.1:0`, pass the **already-bound**
socket to the server (or retry-with-backoff on `EADDRINUSE` and re-pick a port). Removing
the TOCTOU window entirely is preferable to widening a sleep.

## Provenance

Found by the orchestrator while gating `cli-canon-help-json` (2026-08-17). Evidence-based,
not speculative: an actual observed failure with an identified mechanism.

