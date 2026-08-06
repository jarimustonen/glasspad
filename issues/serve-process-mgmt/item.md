---
created: 2026-08-06
updated: 2026-08-06
type: feature
status: in-progress
priority: low
commits:
- hash: 999bf30
  summary: 'feat(serve): process management — stop, GLASSPAD_PORT, pid file'
- hash: e86d6ba
  summary: 'fix(serve): apply /llm-review findings — pid_t wrap, atomic write, honest stop'
---

# Serve process management: stop, GLASSPAD_PORT, PID file

## Description

Deferred process-management niceties for the loopback server, targeted at a 0.2.2 patch (optional polish, no hard gate). Three items: (1) `glasspad stop` — stop a running server; (2) `GLASSPAD_PORT` env var to set the serve port; (3) a PID file at `~/.glasspad/server.pid` so stop/status can find the process. Originally captured under the (now-closed) release-oss epic's round-it-out list.
