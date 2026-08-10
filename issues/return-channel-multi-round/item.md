---
created: 2026-08-10
updated: 2026-08-10
type: feature
status: open
priority: normal
---

# Return channel B2 — multi-round (agent re-renders in response)

## Description

Extend the artifact return channel from one-shot (B1) to multi-round: after a `gp.submit()`, the agent updates the artifact and the user acts again — a conversational UI in one page.

Design: issues/artifact-return-channel/{design,models-comparison}.md (B2 section). Keys off the already-shipped versioned submission record (monotonic `id` + `content_version` in src/submissions.rs) — B1 left this hook deliberately.

Scope:
- Server→shell push to swap artifact content mid-session, reusing the shell's existing live-reload SSE carrier (do NOT invent a new push channel).
- Exchange/session state on the server: lifecycle, GC, per-tenant isolation of live sessions.
- Bind each submission to the content-version/round it answered (reject cross-round spoof — server already computes content_version).
- Reconnect/replay semantics (user reloads mid-exchange).
- SECURITY (gate): pushing new content into a live sandbox is a larger surface. Each round MUST stay in the null-origin frozen sandbox with connect-src 'none'; add Wave cases proving round N cannot inject/escape or open a network channel. ./test-security.sh must stay green (48 checks + Wave 2a) and gain the new round cases.
- Hosted + loopback parity as with B1.

This touches production/security code — run /llm-review (+ /assess-findings) before merging.
