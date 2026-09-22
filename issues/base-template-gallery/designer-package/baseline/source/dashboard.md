# Service health snapshot

Updated 22 September 2026, 09:30 UTC · [Open operating notes](../../samples/board.md)

## Requests served

**184,320**

Up 6.2% from the previous seven-day period.

## Successful responses

**99.94%**

Target: 99.90% or better.

## Median response time

**182 ms**

Down 14 ms from the previous period.

## Open incidents

**2**

One monitored, one awaiting an upstream fix.

## Daily request volume

<div class="gp-chart" id="request-volume" aria-label="Daily request volume chart"></div>
<script>
gp.chart("#request-volume", {
  width: "container",
  height: 240,
  data: { values: [
    { day: "Mon", requests: 24200 }, { day: "Tue", requests: 25800 },
    { day: "Wed", requests: 27100 }, { day: "Thu", requests: 26600 },
    { day: "Fri", requests: 28400 }, { day: "Sat", requests: 25700 },
    { day: "Sun", requests: 26520 }
  ]},
  mark: { type: "line", point: true },
  encoding: {
    x: { field: "day", type: "ordinal", title: null },
    y: { field: "requests", type: "quantitative", title: "Requests", scale: { zero: false } }
  }
});
</script>

## Current attention

> **Upstream image processing:** retries are elevated in one region. User-facing success remains above target while the vendor investigates.

- [x] Confirm retry budget
- [x] Add regional alert
- [ ] Review vendor update at 13:00 UTC

## Endpoint detail

| Endpoint | Requests | Success | p95 | State |
|---|---:|---:|---:|---|
| `/publish` | 82,410 | 99.96% | 410 ms | Healthy |
| `/spaces` | 55,780 | 99.99% | 220 ms | Healthy |
| `/assets` | 39,120 | 99.90% | 680 ms | Watching |
| `/submissions` | 7,010 | 99.87% | 530 ms | Investigating |

Data covers the rolling seven days. Synthetic checks and staff traffic are excluded.
