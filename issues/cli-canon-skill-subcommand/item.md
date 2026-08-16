---
created: 2026-08-16
updated: 2026-08-16
type: improvement
status: open
priority: normal
labels: [cli-canon, tooling]
lane: cli-canon
lane_seq: 40
---

# cli-canon: §15/§16 skill list/print subcommands

## Description


Filed by the `stack-cli-alignment` CLI-surface normalisation (homebase epic), phase 1.
Source: homebase `issues/cli-alignment-audit/analysis.md` (2026-08-10 audit) + live
re-verification 2026-08-16. Canon: `AGENTS-AI-FIRST-CLI.md`. This is a **fix** issue
(the audit + review only recommend); laned in `cli-canon` for a future `/stint-start`.

**Gap (§15/§16) — no `skill` subcommand.**

The tool ships (or should ship) a companion AI operating-manual, but there is no CLI door to
install/inspect it. Agents can't self-provision the skill.

**Do:** add `skill list` / `skill install` / `skill print` (stream content, no side effects),
mirroring ossctl/orchestratectl. If skill files already exist in-repo, wire them through.

**Current state (evidence):** skill flattened to `--install-claude`/`--user` flags; no list/print.
