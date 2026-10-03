---
created: 2026-10-03
updated: 2026-10-03
type: bug
reporter: jari
status: open
priority: normal
---

# Mermaid dependency notices URL returns 404

## Description

The public /_gp/v1/mermaid.NOTICES.txt URL is 404 because the base asset route accepts only lowercase filenames. The bundled JavaScript works, but the notices need to be accessible on hosted and static builds.
