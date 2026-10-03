---
created: 2026-10-03
updated: 2026-10-03
type: bug
reporter: jari
status: fixed
priority: normal
closed: 2026-10-03
closed_by: jari
---

# Mermaid dependency notices URL returns 404

## Description

The public /_gp/v1/mermaid.NOTICES.txt URL is 404 because the base asset route accepts only lowercase filenames. The bundled JavaScript works, but the notices need to be accessible on hosted and static builds.

## Acceptance Criteria

- [x] Lowercase notices URL returns 200 through the real base-asset route.
- [x] A static build includes the renamed notices file.
- [x] The published version is verified on the public host.

## Resolution

### 2026-10-03T19:27:07Z · @jari

0.19.1 is live: loopback and public health checks pass, and Mermaid scripts and notices return HTTP 200.
