---
created: 2026-08-14
updated: 2026-08-14
type: feature
reporter: jari
status: open
priority: normal
labels:
- via:agent-homebase-wrapup
- needs-triage
---

# publish: update a published artifact in place (stable slug) instead of …

## Description

publish: update a published artifact in place (stable slug) instead of minting a new URL each publish

**Observed:** `glasspad publish <file> --markdown` mints a NEW capability slug/URL on every
invocation. `--idempotency-key <k>` returns the FIRST published page for that key (HTTP 200),
so it cannot be used to push an updated file to the same URL — a repeat just re-serves the old
content.

**Use case that hit the gap:** a "living" document (a meeting memo) shared with people by link.
Every time the source markdown is edited and re-published, the shareable link changes, so the
recipients' link goes stale and must be re-sent.

**Expected / requested:** a way to update an already-published artifact in place, keeping the
same `/p/<slug>` URL — e.g. `glasspad publish <file> --update <slug>` (or `--replace`), or an
idempotency-key semantics variant that REPLACES the stored page with the new render rather than
returning the first one.

**Exact commands this session:**
  glasspad publish sales/muistiot/2026-08-06-frondeo-bd-palaveri.md --markdown --title '...' --no-open --json
(ran 3× across edits → 3 different slugs: nmngbhn7…, ydxhpnprn…, 44hgv7qn…)
