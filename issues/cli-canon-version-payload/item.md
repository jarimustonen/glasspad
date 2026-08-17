---
created: 2026-08-16
updated: 2026-08-17
type: improvement
status: in-progress
priority: normal
labels: [cli-canon, tooling]
lane: cli-canon
lane_seq: 20
---

# cli-canon: §10 version --json supported_schemas + skills[]

## Description


Filed by the `stack-cli-alignment` CLI-surface normalisation (homebase epic), phase 1.
Source: homebase `issues/cli-alignment-audit/analysis.md` (2026-08-10 audit) + live
re-verification 2026-08-16. Canon: `AGENTS-AI-FIRST-CLI.md`. This is a **fix** issue
(the audit + review only recommend); laned in `cli-canon` for a future `/stint-start`.

**Gap (§10) — `version --json` payload incomplete.**

The success envelope is correct, but `version --json` lacks `supported_schemas` and
`skills[]`, so agents can't drift-audit schema or skill↔CLI version in one call.

**Do:** add `supported_schemas: [N,…]` and `skills[{name,cli_version,schema_version}]` to the
`version` payload.

**Current state (evidence):** envelope OK but `version --json` lacks supported_schemas and skills[].
