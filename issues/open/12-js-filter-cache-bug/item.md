---
created: 2026-04-09
type: bug
reporter: ai-review
status: open
priority: high
---

# 12. getFilteredDataExcluding() broken cache

_Source: `src/client/dashboard.js` — `getFilteredDataExcluding()`_

## Description

`getFilteredDataExcluding()` has unreachable cache-write code. The function returns directly from `raw.filter(...)`, so `excludeCache[cacheKey] = result` is never reached. The cache never populates, causing repeated O(N) recomputation on every filter change.

## Found by

Codex (gpt-5.4) during markdown section code review, cross-review round 1.

## Fix

Assign filter result to a variable before returning:

```js
var result = raw.filter(function(row) { ... });
excludeCache[cacheKey] = result;
return result;
```
