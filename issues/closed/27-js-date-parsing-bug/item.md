---
created: 2026-04-09
updated: 2026-04-11
type: bug
reporter: ai-review
status: closed
priority: normal
---

# 27. Unchecked Date parsing in hour/timeUnit filters

_Source: `src/client/dashboard.js` — `getFilteredData()`, `getFilteredDataExcluding()`_

## Description

For hour filters, `new Date(hv)` is called without checking parse success. Invalid dates become NaN. Since `NaN < min` and `NaN > max` both evaluate to false, invalid rows incorrectly pass the filter instead of being excluded.

## Found by

Codex (gpt-5.4) during markdown section code review, cross-review round 1.

## Fix

Validate the parsed date before comparison:

```js
var d = new Date(hv);
if (!isFinite(d.getTime())) return false;
var unitVal = extractTimeUnit(d, hRange.timeUnit || 'hours');
if (!isFinite(unitVal) || unitVal < hRange.min || unitVal > hRange.max) return false;
```
