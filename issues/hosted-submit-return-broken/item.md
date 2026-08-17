---
created: 2026-08-14
updated: 2026-08-14
type: bug
status: fixed
priority: high
commits:
- hash: 52e9e4a
  summary: fix return channel for space pages + submissions drain command + publish hint
- hash: 57cdd43
  summary: amended message (clean backticks)
closed: 2026-08-14
---

# Hosted form submissions don't reach the creating agent (return channel dead on share.example.com)

## Description

## Symptom (reported by Jari 2026-08-13)

On **hosted** glasspad (`share.example.com`), a published page that contains **forms / interactive choices does NOT deliver the user's selections back to the creating agent.** The return channel appears dead on hosted — the choices "don't go back to the agents." This bothers real use; hosted should be as interactive as loopback.

## What is already known (orchestrator triage 2026-08-13)

Interactivity infra is present in current code AND live on share.example.com — this is **not** a stale-binary deploy gap:
- base-libs served on share.example.com: `charts.js`, `bridge.js`; `bridge.js` contains the `gp.submit`/`round` code.
- hosted return-channel routes respond (not 404): `POST /api/v1/pages/{slug}/submit` → 403, `/submissions` → 401, `/submissions/stream` → 401 (auth/anti-spoof rejecting an unauthenticated probe, i.e. the routes exist).

So the gap is behavioral, not missing routes. Candidate causes to investigate:
1. **No agent listening.** `gp.submit` on hosted POSTs to `/api/v1/pages/{slug}/submit`, but the selection only reaches an agent if one is holding `glasspad await-submission` (long-poll/SSE) for that slug. A published-and-forgotten page has no consumer → the submit lands in the store but nothing pulls it → "choices don't go back." May be **by-design** (the return channel is an agent↔artifact loop) rather than a bug — clarify.
2. **Hosted submit rejects the shell POST.** The 403 on submit may indicate the hosted submit path's anti-spoof/CSRF/origin/token gate rejects the trusted-shell POST as actually wired on a published page (vs loopback where it works). Compare the loopback submit gate (`src/server.rs` `loopback_submit`, requires loopback Origin + shell token, no API key) against the hosted submit gate (`src/hosted/submit.rs`) — a mismatch in what the published shell sends vs what hosted expects would silently drop submissions.
3. **Shell wiring differs hosted vs loopback.** Check `src/artifact_host/shell.rs` `SUBMIT_PATH` — does a hosted-published page get a correct absolute submit path + token, or is it wired for loopback only?

## Scope of THIS issue: READ-ONLY analysis
Reproduce the symptom against share.example.com (or a local `host-serve`) with a page carrying a `<form>` + `gp.submit()` button, locate the responsible code (Read/Grep only), and classify: **real bug** (hosted submit path drops/rejects shell POSTs), **expected behaviour** (needs an agent long-poll and there was none), or **cannot tell**. Estimate severity and sketch what a fix would touch. Do NOT change application code — the only write is this issue update. The fix/defer decision is the maintainer's.

## Related
- Return channel: `gp.submit` → trusted-shell airlock → server → `glasspad await-submission`. Landed 0.4.0 (loopback+hosted); A2 (SSE) + B2 (multi-round) in 0.5.0.
- Lane B (server/CLI/hosted core): `src/hosted/submit.rs`, `src/server.rs`, `src/artifact_host/shell.rs`.

## Triage analysis (read-only, 2026-08-14)

### Verdict: **primarily EXPECTED BEHAVIOUR — the return channel needs an agent listening; a passively-published page has no consumer.** Not a code defect in the submit path. Secondary risk is a *deployment-config* one (`public-host`), not a code bug.

The three candidate causes from the orchestrator triage resolve as follows.

**Candidate 3 (shell wired for loopback only) — RULED OUT.** The hosted shell emits the correct absolute submit path. `render_with_groups` (`src/artifact_host/shell.rs:154-159`) branches on the server-constant `mount`: loopback (`mount==""`) → `/{space}/_gp/submit`; hosted (`mount=="/p"`) → `/api/v1/pages/{space}/submit`, where on hosted the "space" name **is** the page's capability slug. Proven by the passing unit test `shell_hosted_submit_endpoint_is_api_route` (`shell.rs:961-967`): mount `/p`, slug `abcslug` → `SUBMIT_PATH = "/api/v1/pages/abcslug/submit"`. The shell POSTs same-origin via `fetch(SUBMIT_PATH, …)` (`shell.rs:539`), which the shell CSP's `connect-src 'self'` permits. Wiring is correct for hosted.

**Candidate 2 (hosted submit rejects the shell POST) — RULED OUT as the reported symptom; the 403 the orchestrator saw is expected, not the bug.** The only functional gate on `POST …/submit` is `origin_ok` (`src/hosted/submit.rs:106, 330-335`): it is **fail-closed** — it requires an `Origin` header exactly equal to the server's configured `public_origin`. A `curl`/probe with **no** `Origin` is rejected `403 bad_origin` — that is exactly the CSRF boundary working, and explains the orchestrator's 403. A **real browser** loading the shell from `https://share.example.com` always sends `Origin: https://share.example.com`, which matches the canonical `public_origin` (`validate_public_origin`, `hosted/mod.rs:100-136`, returns `url.origin().ascii_serialization()`). Confirmed by the passing test `submit_then_owner_reads_but_other_tenant_cannot` (`hosted/mod.rs:1284-1326`): a same-origin shell POST → `201 CREATED`, and the owner reads it back over the API-key read route. So a correctly-served hosted page's submit **works**.
  - *Residual config risk (NOT a code bug):* the submit gate demands an **exact** origin-string match. If the server is launched with `--public-host` set to anything other than the exact scheme+host+port the browser actually loads (e.g. `http://` vs `https://`, an added/omitted port, a `www.`/apex mismatch, or fronting by a proxy that changes the visible origin), every real shell POST would 403 and submissions would be silently dropped client-side (the shell only bumps `__bridgeStats.submitFailed`; the user sees nothing). This is verifiable only against the live `share.example.com` process's launch args, not from the code. Worth Jari confirming `--public-host` equals `https://share.example.com` exactly.

**Candidate 1 (no agent listening) — CONFIRMED as the operative cause.** The return channel is an **agent↔artifact loop**, not a fire-and-forget notification. On submit, the handler binds slug+tenant+content-version server-side and persists the payload into the durable on-disk `SubmissionStore` (`hosted/submit.rs:148-162`; store is fsync'd/atomic per `submissions.rs:1-48`) — it lands regardless of whether anyone is connected. But nothing *delivers* it: an agent only receives a submission by actively reading the **API-key-authenticated, owner-scoped** read routes — `…/submissions` (poll), `…/submissions/wait` (long-poll, the default `glasspad await-submission`), or `…/submissions/stream` (SSE) (`hosted/submit.rs:67-75`, consumer in `cli.rs:2801-2868`). A page that was **published and forgotten** has no process holding `await-submission --server https://share.example.com --api-key <key> <slug>`, so the user's choices persist in the store but **nothing pulls them** → "choices don't go back to the agents." Submissions survive for `retention_days` (durable multi-day window; GC in `hosted/mod.rs:293-307, 344-380`), so an agent that returns within retention and polls `…/submissions?since=0` still gets every stored answer — they are not lost, just unconsumed.

### The key finding for Jari
"Choices don't go back" is **by design**: the hosted return channel delivers only to an agent that is actively long-polling / polling / streaming that slug with the page's API key. There is **no push-to-a-departed-agent** and no persistent agent session for a passively-shared page. This is a **UX / documentation / product-model gap, not a code bug in the submit path** — the submit path is correct and the data is stored durably. The gap: a human interacting with a hosted page expects their answer to "reach the agent," but that only happens inside a live agent↔artifact session.

### Severity + who it hits
- **Medium.** Not a crash or data loss (submissions persist for the retention window), but it defeats the core promised interaction ("hosted should be as interactive as loopback") for the most natural hosted workflow: publish a page, share the link, walk away. Hits **any** agent/user who treats a hosted page as an asynchronous form whose results will "arrive later" without keeping a consumer running.
- Loopback feels more interactive only because there the agent and the `serve`/`await-submission` process are typically both live in the same session; the underlying model is identical.

### Affected area / files (no code changed)
- `src/hosted/submit.rs` — submit gate (`origin_ok`, fail-closed) + read routes (auth/owner-scoped). Correct as written.
- `src/artifact_host/shell.rs` — hosted `SUBMIT_PATH` wiring. Correct.
- `src/cli.rs:2801` `await_submission` / `src/main.rs` `AwaitSubmission` — the only delivery surface; requires `--server` + `--api-key`.
- `src/submissions.rs`, `src/hosted/mod.rs` (GC/retention) — durable store; submissions are not lost within retention.

### Repro status
- **Not reproduced against live share.example.com** (no external creds in this read-only worktree).
- **Reproduced by code trace + existing passing tests** that exercise the exact path: `hosted/mod.rs::submit_then_owner_reads_but_other_tenant_cannot` (same-origin POST → 201 → owner reads it back) and `shell.rs::shell_hosted_submit_endpoint_is_api_route` (correct hosted submit URL). Together they show: a correctly-served hosted page's submit succeeds and is stored + readable by the owning agent — so the only way "choices don't go back" is (a) no agent consuming [confirmed cause], or (b) a `public-host` origin-string mismatch [config, unverifiable from code].

### Minor code-hygiene note (not the bug)
`src/hosted/submit.rs:104-105` carries a **stale/contradictory doc comment** claiming "A request with no `Origin` … is allowed," while the actual `origin_ok` (lines 330-335) and its own doc (324-329) are **fail-closed** (missing Origin → rejected). The code is correct (fail-closed CSRF); the inline comment at 104-105 is wrong and should be fixed to avoid future confusion. Cosmetic only.

### Fix sketch (a sketch, not an implementation — disposition is the maintainer's)
This is a **product-model decision**, so pick a lane first:
1. **Docs/UX only (smallest).** Make the agent-loop requirement explicit: when `glasspad publish`/`host` returns a page URL, also print the exact `glasspad await-submission --server <origin> --api-key <key> <slug>` invocation and a one-line "submissions are only delivered while an agent is listening; they persist <retention_days> days otherwise." Touches `src/cli.rs` publish output + hosted `AGENTS.md`. No security surface.
2. **Surface stored-but-unconsumed submissions (small).** A `glasspad submissions <slug> --server … --api-key …` list command (the poll route already exists) so an agent that returns later can drain the backlog with one call, plus documenting `--since 0`. Mostly CLI plumbing over `…/submissions`.
3. **Client-side failure visibility (small, defensive).** Today a rejected submit only bumps `__bridgeStats.submitFailed` invisibly (`shell.rs:546-554`); a hostile-config 403 is silent to the user. Optionally surface a non-intrusive "couldn't deliver" affordance. Touches `shell.rs` (security-sensitive — must not widen the sandbox/CSP; keep it text-only via the existing chrome).
4. **True async delivery (large, out of scope for a bugfix).** A durable agent-notification/webhook model so a departed agent is pinged. This is a design change (new egress/notification surface), not a bug fix — should be its own design issue if wanted.

Recommended if a fix is greenlit: **(1)+(2)** as a single small `bugfix`/docs unit (no security surface), and separately have Jari confirm the live `--public-host` value to close the config risk. (3)/(4) are larger and optional.
