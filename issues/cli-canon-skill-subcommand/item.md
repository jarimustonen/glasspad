---
created: 2026-08-16
updated: 2026-08-17
type: improvement
status: done
priority: normal
labels: [cli-canon, tooling]
lane: cli-canon
lane_seq: 40
commits:
- hash: 6d30d4e76920c444abe99958d5b8230b0e80d7c1
  summary: mark skill subcommand work in progress
- hash: df5ba63f1c039d07b6f5bd27e7212f99a9c84e54
  summary: add skill list, print, and canonical install commands
- hash: 2f6400efba50a9cd93b561bd5e80d0df2892e21f
  summary: apply reviewed install-selection and metadata fixes
closed: 2026-08-17
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
