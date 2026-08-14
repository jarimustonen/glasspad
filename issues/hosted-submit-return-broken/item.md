---
created: 2026-08-14
updated: 2026-08-14
type: bug
status: open
priority: high
---

# Hosted form submissions don't reach the creating agent (return channel dead on maalla.dev)

## Description

## Symptom (reported by Jari 2026-08-13)

On **hosted** glasspad (`glasspad.maalla.dev`), a published page that contains **forms / interactive choices does NOT deliver the user's selections back to the creating agent.** The return channel appears dead on hosted — the choices "don't go back to the agents." This bothers real use; hosted should be as interactive as loopback.

## What is already known (orchestrator triage 2026-08-13)

Interactivity infra is present in current code AND live on maalla.dev — this is **not** a stale-binary deploy gap:
- base-libs served on maalla.dev: `charts.js`, `bridge.js`; `bridge.js` contains the `gp.submit`/`round` code.
- hosted return-channel routes respond (not 404): `POST /api/v1/pages/{slug}/submit` → 403, `/submissions` → 401, `/submissions/stream` → 401 (auth/anti-spoof rejecting an unauthenticated probe, i.e. the routes exist).

So the gap is behavioral, not missing routes. Candidate causes to investigate:
1. **No agent listening.** `gp.submit` on hosted POSTs to `/api/v1/pages/{slug}/submit`, but the selection only reaches an agent if one is holding `glasspad await-submission` (long-poll/SSE) for that slug. A published-and-forgotten page has no consumer → the submit lands in the store but nothing pulls it → "choices don't go back." May be **by-design** (the return channel is an agent↔artifact loop) rather than a bug — clarify.
2. **Hosted submit rejects the shell POST.** The 403 on submit may indicate the hosted submit path's anti-spoof/CSRF/origin/token gate rejects the trusted-shell POST as actually wired on a published page (vs loopback where it works). Compare the loopback submit gate (`src/server.rs` `loopback_submit`, requires loopback Origin + shell token, no API key) against the hosted submit gate (`src/hosted/submit.rs`) — a mismatch in what the published shell sends vs what hosted expects would silently drop submissions.
3. **Shell wiring differs hosted vs loopback.** Check `src/artifact_host/shell.rs` `SUBMIT_PATH` — does a hosted-published page get a correct absolute submit path + token, or is it wired for loopback only?

## Scope of THIS issue: READ-ONLY analysis
Reproduce the symptom against maalla.dev (or a local `host-serve`) with a page carrying a `<form>` + `gp.submit()` button, locate the responsible code (Read/Grep only), and classify: **real bug** (hosted submit path drops/rejects shell POSTs), **expected behaviour** (needs an agent long-poll and there was none), or **cannot tell**. Estimate severity and sketch what a fix would touch. Do NOT change application code — the only write is this issue update. The fix/defer decision is Jari's.

## Related
- Return channel: `gp.submit` → trusted-shell airlock → server → `glasspad await-submission`. Landed 0.4.0 (loopback+hosted); A2 (SSE) + B2 (multi-round) in 0.5.0.
- Lane B (server/CLI/hosted core): `src/hosted/submit.rs`, `src/server.rs`, `src/artifact_host/shell.rs`.
