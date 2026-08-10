---
created: 2026-08-10
updated: 2026-08-10
type: feature
status: open
priority: normal
---

# Return channel: interactive artifact form/input back to the creating agent

## Description

**Design SKETCH awaiting a go/no-go decision — not approved to build.** Full sketch in
[`design.md`](design.md).

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

Needs, before any build: a go/no-go, the loopback-vs-hosted decision, the agent-consumption
model, and new Wave adversarial cases (flood, cross-space spoof, CSP-still-frozen regression).
See design.md → "Open questions for Jari".

