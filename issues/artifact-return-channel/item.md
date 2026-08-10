---
created: 2026-08-10
updated: 2026-08-10
type: feature
status: done
priority: normal
closed: 2026-08-10
---

# Return channel: interactive artifact form/input back to the creating agent

## Description

**Scheduled (own DAG lane). Direction decided 2026-08-10: build for the hosted model;
loopback rides along.** Design in [`design.md`](design.md); transport & interaction-shape
pro/cons in [`models-comparison.md`](models-comparison.md).

Can a Glasspad artifact collect user interaction (form submit, button choice, wizard step)
and return that input to the agent that created the space? Today: **no, by design** — the
artifact runs under `connect-src 'none'` + a null-origin sandbox with no `allow-forms`, so
it has no network egress and native form submit is blocked (this is the `test-security.sh`
exfil boundary).

Core idea in the sketch: **don't relax the artifact sandbox** — route input through the
**trusted shell** as an airlock. Artifact `postMessage`s the shell (already-allowed
channel, as `bridge.js` navigation does), shell validates + POSTs to a new server endpoint
(`connect-src 'self'` already permits it), server delivers to the agent (loopback: JSONL /
`await-submission`; hosted: per-tenant persisted + poll). Because the artifact can only
carry agent-embedded or user-typed data (null origin, no fetch), the channel opens **no new
exfil vector**.

Settled at/before build (see models-comparison.md): the consumption transport (recommendation:
A1 polling + A3 `await-submission`) and one-shot vs multi-round (recommendation: B1 one-shot
with a versioned submission record so SSE + multi-round are additive later). Mandatory: new
Wave adversarial cases (flood, cross-space/cross-round spoof, CSP-still-frozen regression).

