---
created: 2026-08-16
updated: 2026-08-17
type: improvement
status: done
priority: normal
labels: [cli-canon, tooling]
lane: cli-canon
lane_seq: 50
commits:
- hash: 0059ad10c74c08e82825d454d1d859f7b86ad0e6
  summary: add read-only doctor diagnostics
- hash: 70ecc07060fb2e677faa6e1b7e808483e3384fc9
  summary: apply doctor review findings
closed: 2026-08-17
---

# cli-canon: §18 doctor self-diagnostic

## Description


Filed by the `stack-cli-alignment` CLI-surface normalisation (homebase epic), phase 1.
Source: homebase `issues/cli-alignment-audit/analysis.md` (2026-08-10 audit) + live
re-verification 2026-08-16. Canon: `AGENTS-AI-FIRST-CLI.md`. This is a **fix** issue
(the audit + review only recommend); laned in `cli-canon` for a future `/stint-start`.

**Gap (§18) — no `doctor` self-diagnostic.**

Agents have no cheap "is this tool healthy / correctly configured?" probe.

**Do:** add a read-only `doctor` (with `--json` per-check `{id,status,message,fix_suggestion}`
+ a `summary{ok,warn,fail}`, exit 1 on fail; optional `--fix`), matching the example-intake-tool/issuectl
exemplars.

**Current state (evidence):** `glasspad doctor` → unrecognized subcommand.
