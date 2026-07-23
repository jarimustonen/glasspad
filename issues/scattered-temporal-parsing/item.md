---
created: 2026-04-11
updated: 2026-04-11
type: improvement
reporter: ai-review
assignee: jari
status: open
priority: normal
slug: tolerably-wide-book
---

# Centralize temporal parsing in dashboard.js

_Source: `src/client/dashboard.js`_

## Description

Temporal value parsing is duplicated across 8+ callsites with inconsistent coercion and error handling. This is the root cause behind @js-date-parsing-bug (date parsing bug) and will cause similar bugs in the future.

Current patterns scattered throughout the file:

- `Date.parse(v)` for range filters
- `new Date(hv)` for hour/timeUnit filters
- `new Date(v)` for time-unit axis setup
- `new Date(dateStr)` from aria labels
- `typeof v === 'number' ? v : Date.parse(v)` for temporal extents

Each callsite handles nulls, invalid values, and type coercion differently.

## Scope

Introduce shared helpers and use them everywhere:

```js
function toTimestamp(value) {
  if (value == null) return NaN;
  if (typeof value === 'number') return isFinite(value) ? value : NaN;
  var t = Date.parse(value);
  return isFinite(t) ? t : NaN;
}

function toValidDate(value) {
  var t = toTimestamp(value);
  return isFinite(t) ? new Date(t) : null;
}
```

### Callsites to update

- `getFilteredData()` — range filter and hour filter paths
- `getFilteredDataExcluding()` — range filter and hour filter paths
- `temporalExtent()`
- Slider initialization (allUnits collection)
- Chart axis midpoint/tick generation
- `binIndexFromBar()` label parsing
- `dimBarsOutsideRange()`

## Found by

Gemini and Codex (consensus) during @js-date-parsing-bug code review.
