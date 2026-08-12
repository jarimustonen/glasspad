---
created: 2026-08-12
updated: 2026-08-12
type: feature
status: in-progress
priority: high
---

# Publish-first CLI: collapse the surface, config-driven target (loopback|hosted)

## Description

Make **publish the default verb** and drive its target from config, so the standard agent workflow is 'hand glasspad markdown → get a URL', not 'serve on loopback + open'. See design.md in this issue dir for the full design. Big surface reshape (cli.rs/main.rs/config/skill.md); design-first, no back-compat. Touches production/security code — /llm-review before merge; ./test-security.sh must stay green.
