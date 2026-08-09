---
created: 2026-08-09
updated: 2026-08-09
type: bug
status: open
priority: normal
---

# hosted share pages do not emit the documented noindex directive

_Source: src/hosted (read routes / headers)_

## Description

skill.md and the design (Option D / G3 in the glasspad-html-consolidation design) both promise that hosted share pages served at `/p/<slug>` carry `noindex` ("hold the link"). In practice neither an `X-Robots-Tag: noindex` response header nor a `<meta name=robots content=noindex>` tag is emitted on either the shell route (`/p/<slug>/`) or the sandboxed content route (`/p/<slug>/_c/index`).

Verified 2026-08-09 against the live glasspad.maalla.dev deploy: response headers include the frozen CSP, x-frame-options: DENY, referrer-policy: no-referrer, cache-control: no-store — but nothing robots-related. A shared capability URL that leaks to a crawler could therefore be indexed, contrary to the documented 'hold the link, not indexed' contract.

Risk is bounded (unguessable ~50-bit slug, no index route), but the fix is cheap: add `X-Robots-Tag: noindex, nofollow` to the hosted read responses (host-serve mode only — not the loopback serve). Then the behavior matches skill.md + the design.
