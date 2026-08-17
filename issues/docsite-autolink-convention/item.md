---
created: 2026-08-14
updated: 2026-08-17
type: feature
reporter: maintainer
status: done
priority: low
lane: space-polish
lane_seq: 20
commits:
- hash: d2191df
  summary: document producer preprocessing seam and link-class contract
- hash: 18acc11
  summary: clarify custom directory template configuration
closed: 2026-08-17
---

# Documented producer convention for cross-doc autolink + glossary term linking in spaces

_Source: producer-example docs/ port_

## Description

**Motivation:** producer-example's build_docs.py does semantic preprocessing glasspad doesn't (and arguably shouldn't) own: (1) auto-linking glossary terms to the glossary page, (2) xref styling for cross-doc references. glasspad renders plain markdown links only. This is fine as **producer-side preprocessing**, but there's no documented convention for where that fits with a glasspad space, nor a way to style a class of link (xref) via the space theme. **Ask (low priority):** document the recommended 'preprocess markdown before publish' seam for spaces, and confirm/allow a small set of author-supplied link classes to survive into the rendered prose page for theming. Mostly a docs/convention issue; producer-example will keep a thin preprocessor either way. Tracked in producer-example `project-view`.
