---
created: 2026-08-05
updated: 2026-08-06
type: feature
status: done
priority: normal
commits:
- hash: 5c3dd59
  summary: add version subcommand + --version/-V with --json envelope and contract tests
- hash: 6debc79
  summary: address llm-review — honor --json on the flag, nested data envelope, commit field
closed: 2026-08-06
---

# Expose an installed-version command (glasspad --version / version)

## Description

## Problem

glasspad exposes no way to query its own installed version: `glasspad --version`, `glasspad -V`, and `glasspad version` all error (`unexpected argument` / `unrecognized subcommand`). Subcommands are serve/create/open/data/skill/help only.

## Impact

Deployment automation provisions the shared CLIs on Linux via each tool's cargo-dist release installer, version-gated on the latest release tag so a re-run is a cheap no-op. issuectl (`issuectl --version`), ossctl (`ossctl version`), and orchestratectl (`orchestratectl version` → JSON) all expose a version, so their Linux hooks compare installed-vs-latest directly. glasspad does not, so the installer had to fall back to writing a marker file (`~/.local/state/glasspad.installed-tag`) after each install to gate the next run, a workaround the siblings don't need.

## Ask

Add a standard version command — `glasspad --version` (clap's built-in `version` on the top-level command) and/or a `glasspad version` subcommand, ideally with a `--json` envelope like orchestratectl/ossctl. Then the fleet hook can drop the marker workaround and gate like the others.
