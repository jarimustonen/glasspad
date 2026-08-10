---
created: 2026-08-10
updated: 2026-08-10
type: improvement
status: open
priority: normal
blocked_by: ['@return-channel-multi-round']
---

# Return channel A2 — SSE transport for await-submission

## Description

Add a server-push (SSE) delivery transport to the return channel alongside the existing A3 backgrounded long-poll: agent holds an EventSource on GET /api/v1/pages/<slug>/submissions/stream; server pushes each submission as it lands.

Design: issues/artifact-return-channel/models-comparison.md (A2 section). Reuses the shell's existing SSE plumbing (head start — same server-sent-events code path). Value: one agent watching MANY pages at once, or sub-second streaming; the long-poll stays the primary/fallback surface.

Scope:
- New SSE endpoint over the persisted-cursor store (A1 substrate already exists in src/submissions.rs) — since=<id> cursor semantics preserved, no re-deliver/skip.
- await-submission gains an SSE mode (or a sibling command/flag) that consumes the stream; plain long-poll remains the default/fallback.
- Held-connection lifecycle + per-tenant isolation; reconnect resumes from cursor.
- ./test-security.sh green (48 + Wave 2a); add exfil/abuse cases for the streaming endpoint.

BLOCKED BY return-channel-multi-round (B2) — sequenced on the same Lane B hot files (src/server.rs, src/submissions.rs, src/cli.rs). Do NOT start until B2 has landed.

Touches production/security code — run /llm-review (+ /assess-findings) before merging.
