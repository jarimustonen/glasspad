# GUI Debugging Guide

Glasspad renders charts **client-side** with Vega-Lite. This guide covers
how to verify rendering and debug interactions without manual testing.

## Rebuild pipeline

Dashboard JS is embedded at **build time** (`include_str!` in renderer.rs).
After editing `src/client/dashboard.js` you MUST:
1. `cargo build`
2. Restart server
3. Create a **new** pad (old pads serve stale JS)

## Approach 1: Static verification (curl + Python)

Best for: checking computed values (axis labels, tick counts, data transforms).

Fetch the served HTML, parse the `<script>` tags (spec JSON, data JSON,
dashboard.js), and simulate the JS computation in Python.

```bash
curl -s http://localhost:3000/<pad-id> | python3 -c "
import sys, re, json
html = sys.stdin.read()
scripts = re.findall(r'<script[^>]*>(.*?)</script>', html, re.DOTALL)
# Typical layout: CDN libs (empty src), spec JSON, data JSON, dashboard.js
js = [s for s in scripts if 'yourNewCode' in s]
print('FOUND' if js else 'NOT FOUND — stale build?')
"
```

Sanity check: N bars should produce N labels. If you see N+1 labels, they
are at bin boundaries, not bar centers.

## Approach 2: Browser automation (test-browser.sh)

Best for: testing DOM interaction (clicks, drag, filter mode, slider state).

`./test-browser.sh` automates Brave Browser via osascript.

**Requires**: Brave > View > Developer > Allow JavaScript from Apple Events.

```bash
./test-browser.sh deploy          # full rebuild + create pad + navigate
./test-browser.sh errors          # check for visible page errors
./test-browser.sh filter-on       # enter timeline filter mode
./test-browser.sh bar-click 3     # click bar at index 3
./test-browser.sh bar-drag 1 4    # drag from bar 1 to bar 4
./test-browser.sh filter-label    # read current slider label
./test-browser.sh page 'expr'     # eval JS in page context
./test-browser.sh eval 'expr'     # eval JS in isolated context (DOM only)
```

### Isolated world gotcha (critical)

Brave/Chrome run osascript JavaScript in an **isolated world**: the DOM is
shared, but page JavaScript globals are **invisible**. This means:

- `typeof vegaEmbed` → `"undefined"` even though charts render fine
- `window.__yourDebugVar` set by page code → invisible from osascript
- Setting `window.x` from osascript → visible only within osascript

**Fix**: use `./test-browser.sh page 'expr'` — it injects a `<script>`
element that runs in the page context and stores the result in a DOM
data attribute, which osascript can then read back.

If you add `window.__debug = ...` to dashboard.js for debugging, you
**cannot** read it via `eval` — you must use `page`.

### Typical test workflow

```bash
./test-browser.sh deploy          # 1. build + deploy + navigate
./test-browser.sh errors          # 2. check for errors FIRST
./test-browser.sh filter-on       # 3. enter interaction mode
./test-browser.sh bar-click 3     # 4. test interaction
./test-browser.sh filter-label    # 5. verify result
```

## Lessons learned

### Always verify your fix takes effect

"Looks right in code" is not enough:
- `tickBand: "center"` silently does nothing on continuous temporal scales
- `bandPosition: 0.5` — same, only works on band/point scales
- An axis property that's accepted without error may have zero visual effect

### Check the error UI on the page first

Glasspad shows "Chart error: ..." messages visibly on the page. Always run
`./test-browser.sh errors` before deeper debugging. A scope error like
`onBarMouseDown is not defined` was visible on the page while automated
checks missed it.

### vegaEmbed replaces the target element

Event listeners added to `div` before `vegaEmbed(div, ...)` may be lost.
Attach listeners either to the parent `wrapper` element (which survives
vegaEmbed) or inside the `.then()` callback after embed completes.

### Function scope across async boundaries

`function` declarations inside `if` blocks are NOT visible in `.then()`
callbacks that run in the outer scope. Fix: declare `var fn = null` in the
outer scope and assign `fn = function() {...}` inside the block.

### Vega-Lite aria-labels include timeUnit

A bar's aria-label reads `date (year-month-date): Apr 04, 2026`,
NOT `date: Apr 04, 2026`. String matching on field names must handle both
`"field: "` and `"field (timeUnit): "` patterns.
See `extractFieldFromLabel` in dashboard.js.

## Vega-Lite axis reference

- `tickBand` and `bandPosition` → only **band/point** scales, not temporal
- Temporal with `timeUnit` is still a **continuous** time scale underneath
- To center labels under bars: compute bin midpoint timestamps from data
  and set `axis.values` explicitly
- Slider for N bins needs **N+1 boundary stops**, not N
