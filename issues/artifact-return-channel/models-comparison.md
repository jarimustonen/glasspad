# Model comparison — return-channel transport & interaction shape

Companion to [`design.md`](design.md). The architecture (artifact → trusted shell → server
→ agent, shell as airlock, artifact sandbox stays frozen) is settled and **hosted is the
target** (loopback rides along). This doc compares the two still-open choices with pro/cons
and a recommendation:

- **Part A — Consumption transport:** how the *agent* receives submissions from the hosted
  server.
- **Part B — Interaction shape:** one-shot submit vs. multi-round (agent re-renders in
  response).

---

## Part A — Consumption transport (hosted)

The agent is remote from the hosted server and authenticates with its API key (same model as
publish ingest). Four ways it can learn a submission happened. (Loopback is not compared here
— there the agent owns the process, so a local JSONL sink / `await-submission` is trivial and
near-zero-latency regardless of which hosted transport is chosen.)

### A1 — Polling · `GET /api/v1/pages/<slug>/submissions?since=<cursor>`
Agent asks periodically; server returns submissions after the cursor.

- ➕ Simplest server side: stateless, no long-lived connections, scales trivially per-tenant.
- ➕ Reuses the exact API-key auth + per-tenant scoping already built for ingest.
- ➕ Firewall/NAT-proof: agent is always the client. Works for disconnected/batch agents.
- ➕ Durable by nature — submissions persist; a late or restarted agent still gets them.
- ➖ Latency = poll interval; busy-polling wastes requests when nothing arrives.
- ➖ Agent must track a cursor and dedupe (mitigated by a monotonic submission id).

### A2 — Server-push stream · SSE `GET /api/v1/pages/<slug>/submissions/stream`
Agent holds an `EventSource`; server pushes each submission. (The **shell** already uses SSE
for live-reload, so the server-side primitive exists.)

- ➕ Low latency, no wasted requests.
- ➕ Natural fit with the existing SSE plumbing.
- ➖ Long-lived connections **per watching agent** — a real multi-tenant scaling cost.
- ➖ Agent must stay connected + handle reconnute/backfill (needs a `Last-Event-ID` cursor
  anyway, so it carries A1's cursor complexity *plus* connection management).
- ➖ Idle agents holding sockets is the opposite of the batch/disconnected use case.

### A3 — Backgrounded blocking CLI · `glasspad await-submission <slug> --since <cur> --timeout <d>`
**The primary agent-facing model (refined 2026-08-10).** The command blocks on a **server-side
long-poll** (`GET …/submissions/wait?since=<cursor>&timeout=<d>` — the server holds the
connection until a submission lands or the timeout fires) and prints the submission as
`--json` on stdout. The **agent runs it backgrounded**, so it does *not* block: the agent
harness re-invokes the agent with the command's output when the human submits — the answer
arrives as the command's **return value**. (Exactly how this session ran `run wait` /
`gh run watch` in the background.)

- ➕ Best agent ergonomics: fire in the background, get the answer as the command result — no
  polling loop, no cursor bookkeeping, no blocking. AI-first (`--json`, no prompt).
- ➕ No busy-polling (server long-poll) **and** no held agent (backgrounded) — best of A1+A2.
- ➕ Hides cursor/reconnect inside the CLI.
- ➖ Needs `--timeout` (a backgrounded wait that never returns would dangle) → a distinct
  "timed-out, no submission" result so the agent can re-arm or give up.
- ➖ Still rides on the persisted-cursor substrate (below): a submit that lands *between*
  arm calls must be returned on the next `--since`, never missed.

### A4 — Webhook · agent registers a callback URL, server POSTs to it
- ➕ True push, no agent-held connection.
- ➖ Requires the **agent to be publicly reachable** — usually false (agents run local/behind
  NAT). Kills it for the common case.
- ➖ Most machinery: delivery retries, signature verification, endpoint registration/rotation.
- ➖ New outbound-from-server surface to secure.

### Recommendation (Part A)
**A3 (backgrounded `await-submission` over a server-side long-poll) is the primary agent
surface; A1's persisted-cursor store is its durable substrate.** The agent fires
`await-submission` in the background and gets the answer as the command result — no blocking,
no polling loop. The server long-poll (a held `GET …/wait` with a timeout) gives low latency
without busy-polling; the persisted `key→submission` store + monotonic cursor (the A1
mechanism) guarantees a submit that lands between arm calls is delivered on the next
`--since`, not lost. Plain **A1 polling** stays exposed as the disconnected/batch fallback
(same endpoint, no hold). Add **A2 (SSE)** only when one agent must watch *many* pages at once
or wants sub-second streaming — the shell's SSE code is the head start. Skip **A4** unless a
genuinely server-reachable agent shows up.

| | Server cost | Latency | Agent blocking | Reuses existing | Verdict |
|---|---|---|---|---|---|
| A3 await (bg + long-poll) | low (held, bounded) | low | **none (backgrounded)** | ingest auth/scope | **primary surface** |
| A1 poll | low | interval | none | ingest auth/scope | **durable substrate + fallback** |
| A2 SSE | med (held conns) | low | none | shell SSE | later: many-pages / sub-second |
| A4 webhook | high | low | n/a (unreachable) | nothing | skip |

---

## Part B — Interaction shape

### B1 — One-shot submit
Artifact collects input, `gp.submit()` sends it once, agent reads it, the exchange is done.
Corrections mean the agent publishes a new page.

- ➕ Simple + stateless: submission is a terminal event, no session state on server or shell.
- ➕ Matches the concrete near-term use (a hosted approval/choice/form → agent acts).
- ➕ Smallest security surface — server never pushes new content into a live sandbox.
- ➖ No in-place back-and-forth; validation feedback / wizards need a fresh page each step.

### B2 — Multi-round (agent re-renders in response)
After a submit, the agent updates the artifact and the user acts again — a conversational UI
in one page. Requires a **server→shell push to swap artifact content** (the shell's existing
live-reload SSE is the natural carrier) plus exchange/session state.

- ➕ Real interactive UX: wizards, server-side validation with inline correction, progressive
  disclosure, agent-in-the-loop dialogue.
- ➕ The server→shell push primitive already exists (live-reload SSE) — reusable.
- ➖ Much larger security surface: pushing new content into a live sandbox needs its own Wave
  cases (can round N inject/escape? is each round still `connect-src 'none'`?), plus content
  versioning and "which round is this submission for" binding.
- ➖ Session/exchange state on the server (lifecycle, GC, per-tenant isolation of live sessions).
- ➖ Reconnect/replay semantics get real (user reloads mid-exchange).

### Recommendation (Part B)
**Build B1 (one-shot) first; design the submission record so B2 is a clean later increment —
do not preclude it.** Concretely: give every submission a monotonic id and stamp it with the
artifact **content version** it answered (`page slug + content hash/round`). One-shot ignores
the round field; multi-round later keys off it. That one field is the whole forward-compat
cost, and it also hardens B1 (the server can reject a submission whose content version no
longer matches — cross-round spoof protection).

---

## Net recommendation

Hosted, **A3 backgrounded `await-submission` over a server-side long-poll** (primary surface)
backed by **A1's persisted-cursor store** (durable substrate + plain-poll fallback), **B1
one-shot** — with a versioned submission record (monotonic id + answered-content-version) so
**A2 (SSE)** and **B2 (multi-round)** are additive later increments rather than rewrites.
Smallest first cut that delivers the hosted human-in-the-loop round-trip: the agent fires one
backgrounded command and the human's answer comes back as its return value, while the
airlock/frozen-sandbox invariant stays intact.

Open for Jari: agree with the A1+A3 / B1 first cut (and the versioned-record forward-compat),
or weight toward earlier B2 / A2 if a concrete latency- or dialogue-heavy use case is already
in view.
