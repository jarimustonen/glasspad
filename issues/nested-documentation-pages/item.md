---
created: 2026-10-04
updated: 2026-10-04
type: feature
reporter: jari
status: done
priority: normal
closed: 2026-10-04
closed_by: agent
---

# Support nested documentation page paths

## Description

Support docs/architecture/*.md and docs/decisions/*.md as pages with path slugs such as architecture/index, exactly as in native-agent-host/docs/glasspad.yaml. Keep Markdown relative links, shell navigation, hosted publish/update/round trips, static builds, sandbox and existing flat spaces working. Do not ingest unrelated directories or agent metadata.

## Acceptance Criteria

- [x] Publish/serve the real native-agent-host/docs tree: all manifest members exist and both architecture/index and decisions/index open in the shell with working relative links.
- [x] Nested slugs survive ingest, persisted-store reload, update and static build; flat existing slugs and URLs still work.
- [x] Reject path traversal, encoded separators and symlinks in scanned page subdirectories and routed URLs; preserve the frozen artifact CSP/sandbox and capability isolation.
- [x] Cover browser navigation, scanner, hosted and static build with regression tests; full Rust and security gates pass.
