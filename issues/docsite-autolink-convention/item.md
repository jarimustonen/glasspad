---
created: 2026-08-14
updated: 2026-08-16
type: feature
reporter: jari
status: open
priority: low
lane: space-polish
lane_seq: 20
---

# Documented producer convention for cross-doc autolink + glossary term linking in spaces

_Source: aggountant docs/ port_

## Description

**Motivation:** aggountant's build_docs.py does semantic preprocessing glasspad doesn't (and arguably shouldn't) own: (1) auto-linking glossary terms to the glossary page, (2) xref styling for cross-doc references. glasspad renders plain markdown links only. This is fine as **producer-side preprocessing**, but there's no documented convention for where that fits with a glasspad space, nor a way to style a class of link (xref) via the space theme. **Ask (low priority):** document the recommended 'preprocess markdown before publish' seam for spaces, and confirm/allow a small set of author-supplied link classes to survive into the rendered prose page for theming. Mostly a docs/convention issue; aggountant will keep a thin preprocessor either way. Tracked in aggountant `project-view`.
