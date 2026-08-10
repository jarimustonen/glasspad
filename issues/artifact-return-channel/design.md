# Design sketch — artifact → agent return channel

**Status: DESIGN — scheduled (own DAG lane). Direction decided 2026-08-10.** This is how an
interactive Glasspad artifact (forms, buttons, wizard steps) sends user input *back* to the
agent that created the space, without weakening the frozen security contract.

**Decided direction (2026-08-10):**
- **Target = the hosted model.** The return channel is built for hosted share pages
  (`/p/<slug>`, multi-tenant, API-key). The **loopback `serve`** path falls out for free (the
  same shell→server→sink plumbing, with a trivial local sink) and rides along.
- Consumption model (how the agent receives submissions) and one-shot-vs-multi-round are
  compared with pro/cons in a **separate doc**: [`models-comparison.md`](models-comparison.md).
  Those two choices are still open and decided there.

## Goal

Let an agent author an HTML artifact that collects user interaction (a form submit, a
button choice, a multi-step answer) and **receive that input back**, so an agent↔human
round-trip through a rich UI becomes possible — without weakening the "sandboxed artifact
cannot exfiltrate" guarantee that the whole `test-security.sh` suite defends.

## Why it's one-way today (the boundary)

Two document types are served (`src/artifact_host/headers.rs`):

- **Artifact content** — `sandbox allow-scripts allow-top-navigation-by-user-activation`
  (null origin, no `allow-same-origin`, **no `allow-forms`**) + **`connect-src 'none'`**.
  JS runs, but there is *no* network egress (`fetch`/XHR/`sendBeacon`/WS/`EventSource` all
  blocked) and native `<form>` submit is disabled. This is the exfil boundary.
- **Trusted shell** — `connect-src 'self'`, `form-action 'none'`, script only via nonce.
  The shell frames the artifact and already runs an `EventSource` (server→shell live-reload).

The only artifact→shell link today is `bridge.js` postMessaging `{type:"navigate", slug}`
to `window.parent`. postMessage to the parent is **not** blocked by `connect-src 'none'`
(different mechanism) — the bridge already relies on that.

## Key insight — the shell is the airlock; the artifact CSP stays frozen

We do **not** relax the artifact sandbox. The artifact keeps `connect-src 'none'` and no
`allow-forms`. Instead we route input through the trusted shell, which is *already* allowed
to talk to its own origin:

```
[artifact JS]  --postMessage({type:"submit", …})-->  [shell]  --POST /…/submit-->  [server]  -->  [agent]
  connect-src 'none'         (already-allowed channel)     connect-src 'self'          new sink        delivery
```

Three hops, each already or minimally within the model:

1. **Artifact → shell (postMessage).** Extend the bridge protocol with one new inbound
   type, e.g. `{type:"submit", form:"<id>", data:{…}}`. No artifact-CSP change — the bridge
   already postMessages the parent. `bridge.js` grows a small `gp.submit(data)` helper the
   artifact author calls; a native `<form>` is intercepted by the bridge (submit is
   sandbox-blocked, so JS interception is the only path — which conveniently forces all
   input through the one audited helper).
2. **Shell → server (POST).** The shell validates the message (source `=== window.parent`,
   size cap, shape) and POSTs to a new endpoint (`POST /<space>/_gp/submit` for serve,
   `POST /api/v1/pages/<slug>/submit` for hosted). Shell `connect-src 'self'` already
   permits this. The shell is the trusted mediator — it never `eval`s or reflects the payload.
3. **Server → agent (delivery).** The agent that ran the server consumes submissions. Mode-
   dependent (see below).

## Delivery to the agent

Target is **hosted**; loopback is the byproduct. The *transport* (how the agent receives
submissions) is a separate choice compared in [`models-comparison.md`](models-comparison.md).

- **hosted (primary)** — persist submissions per page (per-tenant scoped, exactly like the
  `idempotency_key` mapping just landed), agent retrieves them with its API key. Needs
  retention/GC like pages have. Transport options (poll / SSE-stream / `await-submission`
  wrapper / webhook) → `models-comparison.md`.
- **loopback `serve` (byproduct)** — the agent operates the `glasspad serve` process itself,
  so the same shell→server plumbing lands in a trivial local sink: JSONL file it tails
  (`--submissions-file`), stdout, or `glasspad await-submission`. Near-zero latency, no auth
  needed (loopback). Falls out of the hosted work at near-zero extra cost.

## Security analysis — why this does NOT reopen exfil

The fear is "any outbound path = exfil." It isn't, because of *what data can reach the hop*:

- The artifact has a **null origin** and `connect-src 'none'`, so it can read **nothing** it
  wasn't given — no cookies, no storage, no cross-origin resources, no network. The only
  data it can put in a submission is (a) what the **agent itself embedded** in the HTML, or
  (b) what the **user typed/chose**. Both are exactly the intended payload. So the channel
  carries no third-party secrets — it cannot, by construction.
- The submission goes to the **agent that authored the artifact**, not to an arbitrary third
  party. There is no `connect-src` widening that would let the artifact reach *another* host.
- The shell must treat the payload as **untrusted data** (size-capped, structurally
  validated, never `eval`/`innerHTML`'d) and only accept from its own framed artifact.

### New adversarial cases (would need Wave-level coverage before merge)

- Artifact floods the channel (rate/size limit on shell + server).
- Artifact tries to spoof **another space's** submission (server binds the submission to the
  space/slug from the *shell's* trusted context, never from the artifact payload).
- postMessage spoofing from a non-parent source (shell checks `event.source`).
- Hosted: cross-tenant submission read (scope by API key exactly like pages).
- Confirm the artifact CSP is **still** `connect-src 'none'` + no `allow-forms` after the
  change (regression assert) — the whole design depends on the airlock, not a hole.

## Scope (decided: hosted, loopback rides along)

The build targets **hosted** end-to-end: bridge `gp.submit()` → shell mediator →
`POST /api/v1/pages/<slug>/submit` → per-tenant persisted submissions → agent retrieval
(transport per `models-comparison.md`). The **loopback** sink (JSONL/`await-submission`) is
wired from the same shell→server path and ships alongside. Still-open sub-choices, both in
`models-comparison.md`: the **consumption transport** and **one-shot vs multi-round** (a
richer typed-schema / server-validation protocol is a later increment, not the first cut).

## Rough blast radius (Option A)

- `src/artifact_host/assets/bridge.js` — `gp.submit()` + submit message type (hot file, Lane).
- `src/artifact_host/headers.rs` / `mod.rs` — shell listener + `connect-src 'self'` POST;
  **assert artifact CSP unchanged**.
- `src/server.rs` — new `POST /_gp/submit` route + submissions sink (JSONL/await).
- `src/cli.rs` — `--submissions-file` / `glasspad await-submission` surface.
- `src/skill.md` — document the round-trip pattern for calling agents.
- `test-security.sh` — new Wave cases (flood, spoof, cross-space, CSP-still-frozen).

## Open questions

1. ~~loopback vs hosted~~ — **DECIDED 2026-08-10: hosted is the target; loopback rides along.**
2. Consumption transport (poll / SSE-stream / `await-submission` / webhook) — **open**, pro/cons
   in `models-comparison.md`.
3. One-shot vs multi-round — **open**, pro/cons in `models-comparison.md`.
4. ~~open now or park~~ — **DECIDED: scheduled, own DAG lane.**
