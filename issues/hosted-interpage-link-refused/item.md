---
created: 2026-08-10
updated: 2026-08-10
type: bug
status: fixed
priority: high
labels: [from-homebase]
closed: 2026-08-10
closed_by: claude
---

# Hosted /p/ pages: link to another /p/ page shows 'refused to connect' (sandbox iframe vs X-Frame-Options: DENY)

## Description

## Symptom
On a hosted page, clicking a link that points to ANOTHER hosted page
(`https://share.example.com/p/<slug>/`) yields a blank body with Chrome's
**"share.example.com refused to connect."** The outer shell/header renders; only the
in-frame navigation fails. Real case: a digest index page's "Lue syväluotaus »" link →
its deep-dive page. Repro:
- index shell `…/p/ka3n23d2grsq4g6cterpvttxae/`, its content `/_c/index` contains
  `<a href="https://share.example.com/p/nzmfhhskgustkzlknzktygy6uu/">Lue syväluotaus »</a>`.
- All three URLs return **200 via curl** — the target is served fine.

## Root cause
The content route `/p/<slug>/_c/index` is rendered inside a **sandboxed iframe**. A link in
that content has no `target`, so it navigates **within the iframe** to the target page's
shell. But every hosted shell page is served with `x-frame-options: DENY` and CSP
`frame-ancestors 'none'` — so the browser refuses to load it inside the frame →
"refused to connect." The link itself is correct; the sandbox model breaks page-to-page
navigation.

## Fix options (glasspad-side)
- Inject `<base target="_top">` into the content route, OR add `target="_top"` to rendered
  links, so navigation breaks out of the sandboxed iframe to the top-level tab, AND
- ensure the content iframe sandbox includes `allow-top-navigation-by-user-activation`
  (or `allow-popups` for `target="_blank"`) so the top navigation is actually permitted.
- Consider intercepting same-host `/p/` link clicks in the shell and navigating `window.top`.

Whatever the mechanism: clicking a link to another hosted page must load that page top-level,
not inside the anti-framed sandbox.

## Acceptance
- Clicking a `/p/<slug>/` link from within a hosted page loads that page (top-level), no
  "refused to connect". External links still work. Sandbox isolation for untrusted content
  preserved (don't just drop the anti-framing headers).
- A regression test / fixture covering an inter-page link.

Reported from homebase digest delivery (2026-08-10).
