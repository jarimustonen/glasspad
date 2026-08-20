---
created: 2026-08-18
updated: 2026-08-20
type: feature
status: open
priority: normal
lane: hosted-feature
---

# Report on the page when a submission has not been collected by an agent

## Description

**The product model is settled and is NOT changing: the agent listens.** A hosted page's
return channel delivers only to an agent that is actively polling / long-polling / streaming
that slug with the page's API key. Decided by Jari 2026-08-18: *"Sopimus on, että agentti
kuuntelee. Ei rakenneta mitään erillistä pushia."*

So this issue is **not** push-to-a-departed-agent, and **not** an outbound notification to a
configured URL. Both were considered and rejected. What remains is the honest-feedback gap:

> If nobody is listening, the person who filled in the form is told nothing. Their answer is
> stored durably, but the page implies it was delivered.

**Do:** when a submission is not acknowledged by a consuming agent, say so **on the page**.

## Explicitly out of scope

- Any outbound/webhook call from the glasspad server. Not wanted; do not add one.
- Any push, queue, or callback registration for an agent that has gone away.
- Any change to the agent-listens contract, the submit gate, or retention/GC behaviour.

## Design questions to settle in the worktree

- **What counts as an ack?** Candidates: the submission was returned by any owner-scoped read
  route (`…/submissions`, `…/submissions/wait`, `…/submissions/stream`), or an explicit
  acknowledgement from the agent. Prefer the former if it needs no new agent-side call —
  `glasspad submissions <slug>` (the drain command) already exists and should count.
- **How does the page learn the state?** A same-origin status read from the shell. It must be
  a *pull* from the page, consistent with the existing architecture.
- **What is the timeout / UX?** "Stored, not yet collected" is a legitimate steady state within
  the retention window — the message should not read as an error. Distinguish "waiting" from
  "nobody has collected this in a long time".

## Security constraint (this one is real and does apply)

The status read is reachable from the **published page**, i.e. effectively unauthenticated and
from the null-origin sandbox. It must therefore expose **nothing but the delivery state of the
caller's own submission** — no tenant identity, no other submissions, no counts across pages,
no API-key-scoped data. Fail closed on an unknown or cross-tenant slug exactly as the existing
routes do (opaque 404). `./test-security.sh` must stay green, and this deserves adversarial
coverage since it adds a page-reachable read route.

## Provenance

Closes the open question carried since the `hosted-submit-return-broken` analysis (2026-08-14),
which established that "choices don't go back" is by design, not a defect in the submit path:
submissions are stored durably and are readable by the owning agent within retention; what is
missing is any signal that nobody collected them. Decision recorded 2026-08-18.

