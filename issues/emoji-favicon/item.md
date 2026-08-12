---
created: 2026-08-12
updated: 2026-08-12
type: feature
status: open
priority: normal
---

# Emoji SVG favicon for published spaces (repo emoji via .glasspad.yaml)

## Description

Published (and built) pages should carry a favicon so a hosted glasspad URL shows an icon in the browser tab.

Approach: derive the favicon from an EMOJI as a zero-dependency **SVG favicon** — emit `<svg><text>…emoji…</text></svg>` and reference it via `<link rel="icon" href="…favicon.svg">` in the shell wrap. Modern browsers render it with the OS emoji font; no bundled font, no rasterizer, scales crisply. (PNG/ICO rasterization is a heavier later option only if old-browser .ico support is needed — needs an emoji font + resvg/tiny-skia.)

Emoji source: the repo's own emoji, configured in the new `.glasspad.yaml` repo config (`favicon: 🚀`) from publish-first-surface; fall back to a default glasspad emoji when unset. Applies to hosted publish/publish-space AND build output; each page's sandboxed iframe is unaffected (the favicon is on the outer served document, not inside the artifact sandbox).

Relates to publish-first-surface (shares the .glasspad.yaml config). Keep ./test-security.sh green; /llm-review before merge. Validate the emoji input (reject non-emoji / injection into the SVG — XML-escape).
