---
created: 2026-04-09
updated: 2026-04-09
type: improvement
reporter: jari
assignee: jari
status: open
priority: normal
slug: mildly-sedate-nut
---

# Detail view update on filter change

_Source: list section detail view_
_Epic: **@email-support** Email support_

## Description

When a filter changes while a detail view is open, the detail stays in place as long as the item passes the filter. The detail view should update its context or provide visual feedback when the filtered dataset changes around it.

Currently the detail closes automatically if the selected item is filtered out, but does nothing if the item remains visible while sibling items change.
