# Glasspad

AI-friendly scratchpad for rich data views. Lightweight web service that lets
AI agents create and share visual content (dashboards, charts, interactive UIs)
via a simple API.

## Planning & History

AI-generated planning documents go in `history/` at the repo root.

- `history/TODO.md` -- master-työsuunnitelma
- `history/plan-<topic>.md` -- planning documents
- `history/analysis-<topic>.md` -- research and analysis
- `history/design-<topic>.md` -- design documents
- `history/review-<topic>.md` -- review and audit documents

## Debugging rendered output

Glasspad renders charts client-side with Vega-Lite. You can verify rendering
without a headless browser by fetching the served HTML and simulating the JS
logic.

### Rebuild pipeline

Dashboard JS is embedded at **build time**. After editing
`src/client/dashboard.js`:

1. `cargo build`
2. Restart server (`pkill -f glasspad && cargo run -- serve &`)
3. Create a **new** pad — old pads serve stale JS

### Fetch and inspect

```bash
curl -s http://localhost:3000/<pad-id> | python3 -c "
import sys, re, json
html = sys.stdin.read()
scripts = re.findall(r'<script[^>]*>(.*?)</script>', html, re.DOTALL)
# Typical layout: CDN libs (empty), spec JSON, data JSON, dashboard.js
# Check your code change is present:
js = [s for s in scripts if 'yourNewCode' in s]
print('FOUND' if js else 'NOT FOUND — stale build?')
"
```

### Simulate JS logic in Python

Reproduce the client-side computation in Python and assert expected results.
Example — verifying axis labels match bar count:

```python
# N bars must have N labels (not N+1).
# N+1 labels = labels at bin boundaries, not centers.
assert len(labels) == len(bars), "Labels at boundaries, not centers!"
```

### Vega-Lite axis gotchas

- `tickBand` and `bandPosition` only work on **band/point** scales
- Temporal with `timeUnit` is still a **continuous** scale — these props are ignored
- To center labels on temporal bins: compute midpoint timestamps from data
  and set `axis.values` explicitly
