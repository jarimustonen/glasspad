---
created: 2026-10-04
updated: 2026-10-04
type: feature
reporter: jari
status: open
priority: normal
---

# Support nested documentation page paths

## Description

Support docs/architecture/*.md and docs/decisions/*.md as pages with path slugs such as architecture/index, exactly as in native-agent-host/docs/glasspad.yaml. Keep Markdown relative links, shell navigation, hosted publish/update/round trips, static builds, sandbox and existing flat spaces working. Do not ingest unrelated directories or agent metadata.
